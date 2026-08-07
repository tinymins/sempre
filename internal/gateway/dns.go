package gateway

import (
	"context"
	"fmt"
	"net"
	"net/netip"
	"strings"
	"sync"
	"time"

	"github.com/miekg/dns"
)

type DNSServer struct {
	config  DNSConfig
	servers []*dns.Server
	mu      sync.Mutex
	running bool
	cidrs   []netip.Prefix
}

type DNSDebugResult struct {
	Name     string   `json:"name"`
	Type     string   `json:"type"`
	Upstream string   `json:"upstream"`
	Answers  []string `json:"answers"`
	Detail   string   `json:"detail,omitempty"`
}

func NewDNSServer(config DNSConfig) (*DNSServer, error) {
	config.Enabled = true
	cidrs := []netip.Prefix{}
	for _, value := range config.DomesticCIDRs {
		prefix, err := netip.ParsePrefix(value)
		if err != nil {
			return nil, err
		}
		cidrs = append(cidrs, prefix)
	}
	return &DNSServer{config: config, cidrs: cidrs}, nil
}

func (server *DNSServer) Start() error {
	server.mu.Lock()
	defer server.mu.Unlock()
	if server.running {
		return nil
	}
	handler := dns.HandlerFunc(server.handle)
	for _, host := range server.config.ListenHosts {
		address := net.JoinHostPort(host, fmt.Sprint(server.config.ListenPort))
		for _, network := range []string{"udp", "tcp"} {
			current := &dns.Server{Addr: address, Net: network, Handler: handler, ReadTimeout: 5 * time.Second, WriteTimeout: 5 * time.Second}
			if network == "udp" {
				packet, err := net.ListenPacket("udp4", address)
				if err != nil {
					_ = server.stopUnlocked()
					return fmt.Errorf("listen DNS %s: %w", address, err)
				}
				current.PacketConn = packet
			} else {
				listener, err := net.Listen("tcp4", address)
				if err != nil {
					_ = server.stopUnlocked()
					return fmt.Errorf("listen DNS %s: %w", address, err)
				}
				current.Listener = listener
			}
			server.servers = append(server.servers, current)
			go func(item *dns.Server) { _ = item.ActivateAndServe() }(current)
		}
	}
	server.running = true
	return nil
}

func (server *DNSServer) Stop() error {
	server.mu.Lock()
	defer server.mu.Unlock()
	return server.stopUnlocked()
}

func (server *DNSServer) stopUnlocked() error {
	var failure error
	for _, current := range server.servers {
		if err := current.Shutdown(); err != nil && failure == nil {
			failure = err
		}
	}
	server.servers = nil
	server.running = false
	return failure
}

func (server *DNSServer) Running() bool {
	server.mu.Lock()
	defer server.mu.Unlock()
	return server.running
}

func (server *DNSServer) handle(writer dns.ResponseWriter, request *dns.Msg) {
	response, upstream, err := server.resolve(context.Background(), request)
	if err != nil {
		response = new(dns.Msg)
		response.SetRcode(request, dns.RcodeServerFailure)
		response.RecursionAvailable = true
	} else {
		_ = upstream
	}
	_ = writer.WriteMsg(response)
}

func (server *DNSServer) DebugQuery(ctx context.Context, name, recordType string) (DNSDebugResult, error) {
	queryType := dns.StringToType[strings.ToUpper(valueOr(recordType, "A"))]
	if queryType == 0 {
		return DNSDebugResult{}, fmt.Errorf("unsupported DNS query type %q", recordType)
	}
	message := new(dns.Msg)
	message.SetQuestion(dns.Fqdn(name), queryType)
	response, upstream, err := server.resolve(ctx, message)
	if err != nil {
		return DNSDebugResult{}, err
	}
	answers := []string{}
	for _, answer := range response.Answer {
		answers = append(answers, answer.String())
	}
	return DNSDebugResult{Name: dns.Fqdn(name), Type: dns.TypeToString[queryType], Upstream: upstream, Answers: answers}, nil
}

func (server *DNSServer) resolve(ctx context.Context, request *dns.Msg) (*dns.Msg, string, error) {
	if len(request.Question) == 0 {
		response := new(dns.Msg)
		response.SetReply(request)
		return response, "", nil
	}
	question := request.Question[0]
	if server.config.RejectHTTPS && question.Qtype == dns.TypeHTTPS {
		response := new(dns.Msg)
		response.SetRcode(request, dns.RcodeNameError)
		response.RecursionAvailable = true
		return response, "reject", nil
	}
	if upstream := server.matchRuleUpstream(question.Name); upstream != "" {
		return server.exchange(ctx, request, server.namedUpstream(upstream))
	}
	if server.config.Strategy == DNSStrategyRulesFirst {
		return server.exchange(ctx, request, server.config.RemoteUpstream)
	}
	localResponse, upstream, localErr := server.exchange(ctx, request, server.firstLocalUpstream())
	if localErr == nil && server.domesticResponse(localResponse) {
		return localResponse, upstream, nil
	}
	remoteResponse, remote, remoteErr := server.exchange(ctx, request, server.config.RemoteUpstream)
	if remoteErr == nil {
		return remoteResponse, remote, nil
	}
	if localErr == nil {
		return localResponse, upstream, nil
	}
	return nil, "", remoteErr
}

func (server *DNSServer) exchange(ctx context.Context, request *dns.Msg, upstream string) (*dns.Msg, string, error) {
	client := &dns.Client{Net: "udp", Timeout: 5 * time.Second}
	done := make(chan struct {
		msg *dns.Msg
		err error
	}, 1)
	go func() {
		msg, _, err := client.Exchange(request.Copy(), upstream)
		done <- struct {
			msg *dns.Msg
			err error
		}{msg: msg, err: err}
	}()
	select {
	case <-ctx.Done():
		return nil, upstream, ctx.Err()
	case result := <-done:
		return result.msg, upstream, result.err
	}
}

func (server *DNSServer) firstLocalUpstream() string {
	if len(server.config.LocalUpstreams) == 0 {
		return server.config.RemoteUpstream
	}
	return server.config.LocalUpstreams[0]
}

func (server *DNSServer) namedUpstream(value string) string {
	switch value {
	case "local":
		return server.firstLocalUpstream()
	case "remote", "":
		return server.config.RemoteUpstream
	default:
		return value
	}
}

func (server *DNSServer) matchRuleUpstream(name string) string {
	name = strings.TrimSuffix(strings.ToLower(name), ".")
	for _, set := range server.config.RuleSets {
		if !set.Enabled {
			continue
		}
		for _, rule := range set.Rules {
			if ruleMatchesDomain(rule, name) {
				return set.Upstream
			}
		}
	}
	return ""
}

func ruleMatchesDomain(rule, name string) bool {
	rule = strings.TrimSpace(strings.ToLower(rule))
	if rule == "" || strings.HasPrefix(rule, "#") {
		return false
	}
	kind, payload, ok := strings.Cut(rule, ",")
	if !ok {
		payload = rule
		kind = "domain-suffix"
	}
	payload = strings.TrimSuffix(strings.TrimSpace(payload), ".")
	switch strings.TrimSpace(kind) {
	case "domain":
		return name == payload
	case "domain-keyword":
		return strings.Contains(name, payload)
	default:
		return name == payload || strings.HasSuffix(name, "."+payload)
	}
}

func (server *DNSServer) domesticResponse(message *dns.Msg) bool {
	if message == nil || len(message.Answer) == 0 {
		return false
	}
	for _, answer := range message.Answer {
		var addr netip.Addr
		switch value := answer.(type) {
		case *dns.A:
			addr, _ = netip.AddrFromSlice(value.A)
		default:
			continue
		}
		for _, prefix := range server.cidrs {
			if prefix.Contains(addr) {
				return true
			}
		}
	}
	return false
}
