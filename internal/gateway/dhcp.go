package gateway

import (
	"bytes"
	"encoding/binary"
	"fmt"
	"net"
	"net/netip"
	"strings"
	"sync"
	"time"
)

const (
	dhcpServerPort = 67
	dhcpClientPort = 68
)

type DHCPServer struct {
	config   Config
	conn     *net.UDPConn
	mu       sync.Mutex
	running  bool
	leases   map[string]lease
	stop     chan struct{}
	serverID netip.Addr
}

type lease struct {
	MAC       string
	IP        netip.Addr
	Hostname  string
	ExpiresAt time.Time
	Reserved  bool
}

func NewDHCPServer(config Config) (*DHCPServer, error) {
	serverID, err := gatewayAddr(config)
	if err != nil {
		return nil, err
	}
	return &DHCPServer{config: config, leases: map[string]lease{}, stop: make(chan struct{}), serverID: serverID}, nil
}

func (server *DHCPServer) Start() error {
	server.mu.Lock()
	defer server.mu.Unlock()
	if server.running {
		return nil
	}
	addr := &net.UDPAddr{IP: net.IPv4zero, Port: dhcpServerPort}
	conn, err := net.ListenUDP("udp4", addr)
	if err != nil {
		return fmt.Errorf("listen DHCP: %w", err)
	}
	server.conn = conn
	server.running = true
	go server.serve()
	return nil
}

func (server *DHCPServer) Stop() error {
	server.mu.Lock()
	defer server.mu.Unlock()
	if !server.running {
		return nil
	}
	close(server.stop)
	server.running = false
	err := server.conn.Close()
	server.conn = nil
	server.stop = make(chan struct{})
	return err
}

func (server *DHCPServer) Running() bool {
	server.mu.Lock()
	defer server.mu.Unlock()
	return server.running
}

func (server *DHCPServer) Leases() []LeaseView {
	server.mu.Lock()
	defer server.mu.Unlock()
	result := make([]LeaseView, 0, len(server.leases)+len(server.config.DHCP.Reservations))
	for _, reservation := range server.config.DHCP.Reservations {
		result = append(result, LeaseView{MAC: strings.ToLower(reservation.MAC), IP: reservation.IP, Hostname: reservation.Hostname, Reserved: true})
	}
	now := time.Now().UTC()
	for _, item := range server.leases {
		if now.After(item.ExpiresAt) {
			continue
		}
		expires := item.ExpiresAt
		result = append(result, LeaseView{MAC: item.MAC, IP: item.IP.String(), Hostname: item.Hostname, ExpiresAt: &expires, Reserved: item.Reserved})
	}
	return result
}

func (server *DHCPServer) Revoke(mac string) error {
	server.mu.Lock()
	defer server.mu.Unlock()
	key := strings.ToLower(mac)
	if _, ok := server.leases[key]; !ok {
		return fmt.Errorf("lease for %s not found", mac)
	}
	delete(server.leases, key)
	return nil
}

func (server *DHCPServer) serve() {
	buffer := make([]byte, 1500)
	for {
		count, _, err := server.conn.ReadFromUDP(buffer)
		if err != nil {
			select {
			case <-server.stop:
				return
			default:
				continue
			}
		}
		packet := append([]byte(nil), buffer[:count]...)
		if response, ok := server.response(packet); ok {
			_, _ = server.conn.WriteToUDP(response, &net.UDPAddr{IP: net.IPv4bcast, Port: dhcpClientPort})
		}
	}
}

func (server *DHCPServer) response(packet []byte) ([]byte, bool) {
	if len(packet) < 240 || packet[0] != 1 || !bytes.Equal(packet[236:240], []byte{99, 130, 83, 99}) {
		return nil, false
	}
	options := parseDHCPOptions(packet[240:])
	messageType := firstOptionByte(options[53])
	if messageType != 1 && messageType != 3 {
		return nil, false
	}
	hardwareLength := int(packet[2])
	if hardwareLength <= 0 || hardwareLength > 16 || 28+hardwareLength > len(packet) {
		return nil, false
	}
	mac := net.HardwareAddr(packet[28 : 28+hardwareLength]).String()
	hostname := string(options[12])
	ip, ok := server.allocate(mac, hostname)
	if !ok {
		return nil, false
	}
	reply := make([]byte, 240, 300)
	copy(reply, packet[:240])
	reply[0] = 2
	copy(reply[16:20], ip.AsSlice())
	copy(reply[20:24], server.serverID.AsSlice())
	reply[236], reply[237], reply[238], reply[239] = 99, 130, 83, 99
	reply = appendOption(reply, 53, []byte{map[byte]byte{1: 2, 3: 5}[messageType]})
	reply = appendOption(reply, 54, server.serverID.AsSlice())
	reply = appendOption(reply, 1, net.CIDRMask(prefixBits(server.config.LAN.GatewayCIDR), 32))
	reply = appendOption(reply, 3, server.serverID.AsSlice())
	if server.config.DNS.Enabled {
		reply = appendOption(reply, 6, server.serverID.AsSlice())
	}
	if server.config.DHCP.Domain != "" {
		reply = appendOption(reply, 15, []byte(server.config.DHCP.Domain))
	}
	leaseSeconds := uint32(server.leaseDuration().Seconds())
	leaseBytes := make([]byte, 4)
	binary.BigEndian.PutUint32(leaseBytes, leaseSeconds)
	reply = appendOption(reply, 51, leaseBytes)
	reply = append(reply, 255)
	return reply, true
}

func (server *DHCPServer) allocate(mac, hostname string) (netip.Addr, bool) {
	server.mu.Lock()
	defer server.mu.Unlock()
	key := strings.ToLower(mac)
	for _, reservation := range server.config.DHCP.Reservations {
		if strings.EqualFold(reservation.MAC, mac) {
			ip, err := netip.ParseAddr(reservation.IP)
			return ip, err == nil
		}
	}
	now := time.Now().UTC()
	if item, ok := server.leases[key]; ok && now.Before(item.ExpiresAt) {
		return item.IP, true
	}
	start, _ := netip.ParseAddr(server.config.DHCP.RangeStart)
	end, _ := netip.ParseAddr(server.config.DHCP.RangeEnd)
	for current := start; current.Compare(end) <= 0; current = current.Next() {
		if server.ipInUse(current) {
			continue
		}
		server.leases[key] = lease{MAC: key, IP: current, Hostname: hostname, ExpiresAt: now.Add(server.leaseDuration())}
		return current, true
	}
	return netip.Addr{}, false
}

func (server *DHCPServer) ipInUse(ip netip.Addr) bool {
	for _, item := range server.leases {
		if item.IP == ip && time.Now().UTC().Before(item.ExpiresAt) {
			return true
		}
	}
	for _, reservation := range server.config.DHCP.Reservations {
		reserved, err := netip.ParseAddr(reservation.IP)
		if err == nil && reserved == ip {
			return true
		}
	}
	return false
}

func (server *DHCPServer) leaseDuration() time.Duration {
	duration, err := time.ParseDuration(server.config.DHCP.LeaseTime)
	if err != nil || duration <= 0 {
		return 12 * time.Hour
	}
	return duration
}

func parseDHCPOptions(data []byte) map[byte][]byte {
	result := map[byte][]byte{}
	for index := 0; index < len(data); {
		code := data[index]
		index++
		if code == 255 {
			break
		}
		if code == 0 || index >= len(data) {
			continue
		}
		length := int(data[index])
		index++
		if index+length > len(data) {
			break
		}
		result[code] = append([]byte(nil), data[index:index+length]...)
		index += length
	}
	return result
}

func firstOptionByte(value []byte) byte {
	if len(value) == 0 {
		return 0
	}
	return value[0]
}

func appendOption(packet []byte, code byte, value []byte) []byte {
	packet = append(packet, code, byte(len(value)))
	return append(packet, value...)
}

func prefixBits(value string) int {
	prefix, err := netip.ParsePrefix(value)
	if err != nil {
		return 24
	}
	return prefix.Bits()
}
