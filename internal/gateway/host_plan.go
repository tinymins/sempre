package gateway

import (
	"bytes"
	"context"
	"crypto/sha256"
	"encoding/base64"
	"fmt"
	"net"
	"net/netip"
	"os"
	"os/exec"
	"strings"
	"time"

	"golang.org/x/crypto/ssh"
)

type HostPlanRequest struct {
	Config Config `json:"config"`
}

type HostPlan struct {
	Topology   string   `json:"topology"`
	Summary    string   `json:"summary"`
	Warnings   []string `json:"warnings"`
	Commands   []string `json:"commands"`
	Persistent []string `json:"persistent_commands"`
	ApplyBySSH bool     `json:"apply_by_ssh"`
	Output     []string `json:"output,omitempty"`
}

type HostApplyRequest struct {
	Config     Config `json:"config"`
	Confirm    bool   `json:"confirm"`
	PrivateKey string `json:"private_key,omitempty"`
}

func BuildHostPlan(config Config) (HostPlan, error) {
	config.Normalize()
	if err := config.Validate(); err != nil {
		return HostPlan{}, err
	}
	prefix, _ := netip.ParsePrefix(config.LAN.GatewayCIDR)
	address := prefix.Addr().String()
	interfaceName := valueOr(config.LAN.Interface, "<lan-interface>")
	wan := valueOr(config.LAN.WANInterface, "<wan-interface>")
	commands := []string{
		fmt.Sprintf("ip addr replace %s dev %s", config.LAN.GatewayCIDR, interfaceName),
		fmt.Sprintf("ip link set %s up", interfaceName),
		"sysctl -w net.ipv4.ip_forward=1",
	}
	persistent := []string{
		"printf 'net.ipv4.ip_forward=1\\n' >/etc/sysctl.d/99-sempre-gateway.conf",
	}
	warnings := []string{}
	if config.LAN.NATEnabled {
		commands = append(commands,
			"nft add table inet sempre_gateway_pve 2>/dev/null || true",
			"nft 'add chain inet sempre_gateway_pve postrouting { type nat hook postrouting priority srcnat; policy accept; }' 2>/dev/null || true",
			fmt.Sprintf("nft add rule inet sempre_gateway_pve postrouting oifname %q ip saddr %s masquerade", wan, prefix.Masked().String()),
		)
		persistent = append(persistent, "# Persist nftables according to the host policy, for example via /etc/nftables.conf.")
	} else {
		warnings = append(warnings, "NAT is disabled; upstream routing must already know the LAN prefix.")
	}
	if config.DHCP.Enabled || config.DNS.Enabled {
		warnings = append(warnings, fmt.Sprintf("VMs should use %s as default gateway and DNS server.", address))
	}
	summary := fmt.Sprintf("Prepare %s with gateway %s on %s", config.Topology, config.LAN.GatewayCIDR, interfaceName)
	return HostPlan{Topology: config.Topology, Summary: summary, Warnings: warnings, Commands: commands, Persistent: persistent, ApplyBySSH: config.Topology == TopologyRemotePVE}, nil
}

func ApplyHostPlan(ctx context.Context, request HostApplyRequest) (HostPlan, error) {
	plan, err := BuildHostPlan(request.Config)
	if err != nil {
		return HostPlan{}, err
	}
	if !request.Confirm {
		return HostPlan{}, fmt.Errorf("host apply requires confirmation")
	}
	commands := append([]string{}, plan.Commands...)
	if request.Config.PVE.ApplyPersistent {
		commands = append(commands, plan.Persistent...)
	}
	if request.Config.Topology == TopologyRemotePVE {
		output, err := runSSHCommands(ctx, request.Config.PVE, request.PrivateKey, commands)
		plan.Output = output
		return plan, err
	}
	output, err := runLocalCommands(ctx, commands)
	plan.Output = output
	return plan, err
}

func valueOr(value, fallback string) string {
	if strings.TrimSpace(value) == "" {
		return fallback
	}
	return value
}

func runLocalCommands(ctx context.Context, commands []string) ([]string, error) {
	output := []string{}
	for _, command := range commands {
		current := exec.CommandContext(ctx, "sh", "-c", command)
		data, err := current.CombinedOutput()
		text := strings.TrimSpace(string(data))
		if text != "" {
			output = append(output, "$ "+command+"\n"+text)
		} else {
			output = append(output, "$ "+command)
		}
		if err != nil {
			return output, fmt.Errorf("run %q: %w", command, err)
		}
	}
	return output, nil
}

func runSSHCommands(ctx context.Context, config PVEConfig, inlineKey string, commands []string) ([]string, error) {
	if strings.TrimSpace(config.Host) == "" {
		return nil, fmt.Errorf("PVE host is required for SSH apply")
	}
	keyData := []byte(strings.TrimSpace(inlineKey))
	if len(keyData) == 0 && strings.TrimSpace(config.KeyPath) != "" {
		data, err := os.ReadFile(config.KeyPath)
		if err != nil {
			return nil, fmt.Errorf("read SSH key: %w", err)
		}
		keyData = data
	}
	if len(keyData) == 0 {
		return nil, fmt.Errorf("SSH private key is required")
	}
	signer, err := ssh.ParsePrivateKey(keyData)
	if err != nil {
		return nil, fmt.Errorf("parse SSH key: %w", err)
	}
	clientConfig := &ssh.ClientConfig{
		User:            valueOr(config.User, "root"),
		Auth:            []ssh.AuthMethod{ssh.PublicKeys(signer)},
		HostKeyCallback: hostKeyCallback(config.Fingerprint),
		Timeout:         10 * time.Second,
	}
	address := net.JoinHostPort(config.Host, fmt.Sprint(config.Port))
	client, err := ssh.Dial("tcp", address, clientConfig)
	if err != nil {
		return nil, fmt.Errorf("connect PVE host: %w", err)
	}
	defer client.Close()
	output := []string{}
	for _, command := range commands {
		if err := ctx.Err(); err != nil {
			return output, err
		}
		session, err := client.NewSession()
		if err != nil {
			return output, err
		}
		var buffer bytes.Buffer
		session.Stdout = &buffer
		session.Stderr = &buffer
		err = session.Run(command)
		_ = session.Close()
		text := strings.TrimSpace(buffer.String())
		if text != "" {
			output = append(output, "$ "+command+"\n"+text)
		} else {
			output = append(output, "$ "+command)
		}
		if err != nil {
			return output, fmt.Errorf("run remote %q: %w", command, err)
		}
	}
	return output, nil
}

func hostKeyCallback(expected string) ssh.HostKeyCallback {
	expected = strings.TrimSpace(expected)
	if expected == "" {
		return ssh.InsecureIgnoreHostKey()
	}
	return func(_ string, _ net.Addr, key ssh.PublicKey) error {
		actual := sshFingerprint(key)
		if actual != expected {
			return fmt.Errorf("PVE host key fingerprint mismatch: got %s", actual)
		}
		return nil
	}
}

func sshFingerprint(key ssh.PublicKey) string {
	sum := sha256.Sum256(key.Marshal())
	return "SHA256:" + base64.RawStdEncoding.EncodeToString(sum[:])
}
