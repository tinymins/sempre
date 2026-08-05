//go:build linux && integration

package transparentproxy

import (
	"context"
	"errors"
	"fmt"
	"net"
	"os"
	"os/exec"
	"slices"
	"syscall"
	"testing"
	"time"

	"github.com/google/nftables"
	"github.com/google/nftables/expr"
	"golang.org/x/sys/unix"
)

const (
	netnsProxyPort = 17893
	netnsDNSPort   = 11053
)

func TestNetNSTProxyDataPlane(t *testing.T) {
	if os.Getenv("SEMPRE_NETNS_TEST") != "1" {
		t.Skip("set SEMPRE_NETNS_TEST=1 and run inside the prepared gateway network namespace")
	}

	ctx := context.Background()
	backend := linuxBackend{}
	if err := backend.Cleanup(ctx); err != nil {
		t.Fatalf("clean initial TProxy state: %v", err)
	}
	proxyTCP := newTransparentTCPListener(t, netnsProxyPort)
	dnsTCP := newTransparentTCPListener(t, netnsDNSPort)
	proxyUDP := newTransparentUDPListener(t, netnsProxyPort)
	dnsUDP := newTransparentUDPListener(t, netnsDNSPort)
	createUnrelatedNFTablesTable(t)

	inventory, err := backend.Inventory(ctx)
	if err != nil {
		t.Fatal(err)
	}
	if !slices.Contains(inventory.RecommendedLANInterfaces, "vmbr1") {
		t.Fatalf("PVE bridge was not recommended as LAN: %#v", inventory.RecommendedLANInterfaces)
	}
	if slices.Contains(inventory.RecommendedLANInterfaces, "docker0") {
		t.Fatalf("Docker bridge was recommended as LAN: %#v", inventory.RecommendedLANInterfaces)
	}
	if !slices.Contains(inventory.VPNPrefixes, "100.80.0.0/16") {
		t.Fatalf("VPN route was not detected: %#v", inventory.VPNPrefixes)
	}

	plan := Plan{
		Mode:             "tproxy",
		TProxyPort:       netnsProxyPort,
		DNSPort:          netnsDNSPort,
		CaptureHost:      true,
		LANInterfaces:    []string{"lan0"},
		ExcludedPrefixes: normalizedPrefixes(append(reservedPrefixes(), inventory.LocalPrefixes...)),
	}
	foreign := createNFTablesTable(t, nftables.TableFamilyIPv4, nftTableName)
	if err := backend.Cleanup(ctx); err != nil {
		t.Fatal(err)
	}
	if !hasNFTablesTable(t, nftTableName) {
		t.Fatal("cleanup removed a foreign table with the Sempre table name")
	}
	if err := backend.ApplyTProxy(ctx, plan); err == nil {
		t.Fatal("TProxy apply accepted a foreign table with the Sempre table name")
	}
	deleteNFTablesTable(t, foreign)
	if err := backend.ApplyTProxy(ctx, plan); err != nil {
		t.Fatal(err)
	}
	cleaned := false
	t.Cleanup(func() {
		if !cleaned {
			_ = backend.Cleanup(context.Background())
		}
	})
	if err := backend.VerifyTProxy(ctx, plan); err != nil {
		t.Fatalf("verify applied TProxy state: %v", err)
	}

	assertTCPIntercepted(t, "203.0.113.10:443", proxyTCP)
	assertUDPIntercepted(t, "203.0.113.10:443", "host-proxy", proxyUDP)
	assertTCPIntercepted(t, "8.8.8.8:53", dnsTCP)
	assertUDPIntercepted(t, "8.8.8.8:53", "host-dns", dnsUDP)

	runNetNSClient(t, "tcp", "203.0.113.11:443", "")
	waitForTCPDestination(t, proxyTCP, "203.0.113.11:443")
	runNetNSClient(t, "udp", "203.0.113.11:443", "lan-proxy")
	waitForUDPPayload(t, proxyUDP, "lan-proxy")
	runNetNSClient(t, "tcp", "8.8.4.4:53", "")
	waitForTCPDestination(t, dnsTCP, "8.8.4.4:53")
	runNetNSClient(t, "udp", "8.8.4.4:53", "lan-dns")
	waitForUDPPayload(t, dnsUDP, "lan-dns")
	// The gateway address belongs to an excluded LAN prefix, but DNS capture
	// must take precedence so clients can use their default gateway as DNS.
	runNetNSClient(t, "tcp", "10.10.10.1:53", "")
	waitForTCPDestination(t, dnsTCP, "10.10.10.1:53")
	runNetNSClient(t, "udp", "10.10.10.1:53", "lan-gateway-dns")
	waitForUDPPayload(t, dnsUDP, "lan-gateway-dns")
	observation, err := readTrafficObservation()
	if err != nil {
		t.Fatal(err)
	}
	if observation.host == 0 || observation.lan == 0 || observation.dns == 0 {
		t.Fatalf("incomplete TProxy traffic counters: %#v", observation)
	}

	if err := backend.Cleanup(ctx); err != nil {
		t.Fatal(err)
	}
	cleaned = true
	if hasNFTablesTable(t, nftTableName) {
		t.Fatalf("Sempre nftables table %q remains after cleanup", nftTableName)
	}
	if !hasNFTablesTable(t, "user_keep") {
		t.Fatal("cleanup removed an unrelated nftables table")
	}
	if err := verifyPolicyRoutes(); err == nil {
		t.Fatal("Sempre policy routes remain after cleanup")
	}
}

func TestNetNSClientHelper(t *testing.T) {
	if os.Getenv("SEMPRE_NETNS_CLIENT") != "1" {
		t.Skip("network namespace helper")
	}
	target := os.Getenv("SEMPRE_NETNS_TARGET")
	switch os.Getenv("SEMPRE_NETNS_PROTOCOL") {
	case "tcp":
		connection, err := net.DialTimeout("tcp4", target, 3*time.Second)
		if err != nil {
			t.Fatal(err)
		}
		defer connection.Close()
		_ = connection.SetReadDeadline(time.Now().Add(3 * time.Second))
		buffer := make([]byte, 1)
		if _, err := connection.Read(buffer); err != nil {
			t.Fatal(err)
		}
	case "udp":
		connection, err := net.DialTimeout("udp4", target, 3*time.Second)
		if err != nil {
			t.Fatal(err)
		}
		defer connection.Close()
		if _, err := connection.Write([]byte(os.Getenv("SEMPRE_NETNS_PAYLOAD"))); err != nil {
			t.Fatal(err)
		}
	default:
		t.Fatalf("unsupported helper protocol %q", os.Getenv("SEMPRE_NETNS_PROTOCOL"))
	}
}

func newTransparentTCPListener(t *testing.T, port int) <-chan string {
	t.Helper()
	listenConfig := transparentListenConfig()
	listener, err := listenConfig.Listen(context.Background(), "tcp4", fmt.Sprintf("0.0.0.0:%d", port))
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = listener.Close() })
	destinations := make(chan string, 16)
	go func() {
		for {
			connection, acceptErr := listener.Accept()
			if acceptErr != nil {
				return
			}
			destinations <- connection.LocalAddr().String()
			_, _ = connection.Write([]byte{1})
			_ = connection.Close()
		}
	}()
	return destinations
}

func newTransparentUDPListener(t *testing.T, port int) <-chan string {
	t.Helper()
	listenConfig := transparentListenConfig()
	connection, err := listenConfig.ListenPacket(context.Background(), "udp4", fmt.Sprintf("0.0.0.0:%d", port))
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = connection.Close() })
	payloads := make(chan string, 16)
	go func() {
		buffer := make([]byte, 1024)
		for {
			count, _, readErr := connection.ReadFrom(buffer)
			if readErr != nil {
				return
			}
			payloads <- string(buffer[:count])
		}
	}()
	return payloads
}

func transparentListenConfig() net.ListenConfig {
	return net.ListenConfig{Control: func(_, _ string, raw syscall.RawConn) error {
		var socketErr error
		if err := raw.Control(func(descriptor uintptr) {
			if err := unix.SetsockoptInt(int(descriptor), unix.SOL_IP, unix.IP_TRANSPARENT, 1); err != nil {
				socketErr = err
				return
			}
			socketErr = unix.SetsockoptInt(int(descriptor), unix.SOL_SOCKET, unix.SO_MARK, int(BypassMark))
		}); err != nil {
			return err
		}
		return socketErr
	}}
}

func assertTCPIntercepted(t *testing.T, target string, destinations <-chan string) {
	t.Helper()
	connection, err := net.DialTimeout("tcp4", target, 3*time.Second)
	if err != nil {
		logNFTablesCounters(t)
		t.Fatal(err)
	}
	defer connection.Close()
	_ = connection.SetReadDeadline(time.Now().Add(3 * time.Second))
	buffer := make([]byte, 1)
	if _, err := connection.Read(buffer); err != nil {
		t.Fatal(err)
	}
	waitForTCPDestination(t, destinations, target)
}

func logNFTablesCounters(t *testing.T) {
	t.Helper()
	connection := &nftables.Conn{}
	chains, err := connection.ListChains()
	if err != nil {
		t.Logf("list nftables chains: %v", err)
		return
	}
	for _, chain := range chains {
		if chain.Table == nil || chain.Table.Name != nftTableName {
			continue
		}
		rules, err := connection.GetRules(chain.Table, chain)
		if err != nil {
			t.Logf("list nftables rules for %s/%s: %v", chain.Table.Name, chain.Name, err)
			continue
		}
		for index, rule := range rules {
			for _, expression := range rule.Exprs {
				if counter, ok := expression.(*expr.Counter); ok {
					t.Logf("nftables %d/%s rule %d: %d packet(s), %d byte(s)", chain.Table.Family, chain.Name, index, counter.Packets, counter.Bytes)
				}
			}
		}
	}
}

func assertUDPIntercepted(t *testing.T, target, payload string, payloads <-chan string) {
	t.Helper()
	connection, err := net.DialTimeout("udp4", target, 3*time.Second)
	if err != nil {
		t.Fatal(err)
	}
	defer connection.Close()
	if _, err := connection.Write([]byte(payload)); err != nil {
		t.Fatal(err)
	}
	waitForUDPPayload(t, payloads, payload)
}

func waitForTCPDestination(t *testing.T, destinations <-chan string, want string) {
	t.Helper()
	deadline := time.NewTimer(3 * time.Second)
	defer deadline.Stop()
	for {
		select {
		case current := <-destinations:
			if current == want {
				return
			}
		case <-deadline.C:
			t.Fatalf("transparent TCP listener did not receive destination %s", want)
		}
	}
}

func waitForUDPPayload(t *testing.T, payloads <-chan string, want string) {
	t.Helper()
	select {
	case current := <-payloads:
		if current != want {
			t.Fatalf("transparent UDP listener received %q, want %q", current, want)
		}
	case <-time.After(3 * time.Second):
		t.Fatalf("transparent UDP listener did not receive %q", want)
	}
}

func runNetNSClient(t *testing.T, protocol, target, payload string) {
	t.Helper()
	binary, err := os.Executable()
	if err != nil {
		t.Fatal(err)
	}
	clientNamespace := os.Getenv("SEMPRE_CLIENT_NETNS")
	command := exec.Command("ip", "netns", "exec", clientNamespace, binary, "-test.run=^TestNetNSClientHelper$")
	command.Env = append(os.Environ(),
		"SEMPRE_NETNS_CLIENT=1",
		"SEMPRE_NETNS_PROTOCOL="+protocol,
		"SEMPRE_NETNS_TARGET="+target,
		"SEMPRE_NETNS_PAYLOAD="+payload,
	)
	if output, err := command.CombinedOutput(); err != nil {
		t.Fatalf("run client helper: %v\n%s", err, output)
	}
}

func createUnrelatedNFTablesTable(t *testing.T) {
	t.Helper()
	table := createNFTablesTable(t, nftables.TableFamilyINet, "user_keep")
	t.Cleanup(func() {
		deleteNFTablesTable(t, table)
	})
}

func createNFTablesTable(t *testing.T, family nftables.TableFamily, name string) *nftables.Table {
	t.Helper()
	connection := &nftables.Conn{}
	table := connection.AddTable(&nftables.Table{Family: family, Name: name})
	if err := connection.Flush(); err != nil {
		t.Fatal(err)
	}
	return table
}

func deleteNFTablesTable(t *testing.T, table *nftables.Table) {
	t.Helper()
	connection := &nftables.Conn{}
	connection.DelTable(table)
	if err := connection.Flush(); err != nil && !errors.Is(err, unix.ENOENT) {
		t.Fatal(err)
	}
}

func hasNFTablesTable(t *testing.T, name string) bool {
	t.Helper()
	connection := &nftables.Conn{}
	tables, err := connection.ListTables()
	if err != nil {
		t.Fatal(err)
	}
	for _, table := range tables {
		if table.Name == name {
			return true
		}
	}
	return false
}
