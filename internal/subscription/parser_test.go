package subscription

import (
	"encoding/base64"
	"strings"
	"testing"
)

func TestParseYAMLRetainsExtraFieldsAndDiscardsPlaceholders(t *testing.T) {
	result := Parse("proxies:\n  - name: usable\n    type: ss\n    server: proxy.example.com\n    port: 443\n    cipher: aes-128-gcm\n    password: secret\n    unknown: retained\n  - name: expired\n    type: ss\n    server: 127.0.0.1\n    port: 1\n")
	if result.Format != "yaml" || len(result.Nodes) != 1 || len(result.DiscardedPlaceholders) != 1 {
		t.Fatalf("unexpected result: %#v", result)
	}
	if result.Nodes[0].Extra["unknown"] != "retained" {
		t.Fatalf("extra fields were not retained: %#v", result.Nodes[0])
	}
}

func TestParseBase64MatchesToolboxSchemes(t *testing.T) {
	encoded := base64.RawURLEncoding.EncodeToString([]byte("vless://id@example.com:443?security=tls&sni=example.com#node\nssr://unsupported"))
	result := Parse(encoded)
	if result.Format != "base64" || len(result.Nodes) != 1 {
		t.Fatalf("unexpected result: %#v", result)
	}
	if !strings.Contains(strings.Join(result.Diagnostics, "\n"), "unsupported") {
		t.Fatalf("missing unsupported diagnostic: %#v", result.Diagnostics)
	}
	if result.Nodes[0].Name != "node" || result.Nodes[0].Type != "vless" {
		t.Fatalf("unexpected proxy: %#v", result.Nodes[0])
	}
}

func TestParseJSONClashDocument(t *testing.T) {
	result := Parse(`{"proxies":[{"name":"json","type":"socks5","server":"example.com","port":1080,"username":"u"}]}`)
	if len(result.Nodes) != 1 || result.Nodes[0].Extra["username"] != "u" {
		t.Fatalf("unexpected result: %#v", result)
	}
}

func TestProtocolURIParametersMatchToolboxSemantics(t *testing.T) {
	vless, err := ParseURI("vless://id@example.com:443?security=reality&sni=edge.example.com&pbk=key&sid=short&insecure=1#vless")
	if err != nil {
		t.Fatal(err)
	}
	if vless.Extra["servername"] != "edge.example.com" || vless.Extra["skip-cert-verify"] != nil || vless.Extra["sni"] != nil {
		t.Fatalf("vless extras = %#v", vless.Extra)
	}

	trojan, err := ParseURI("trojan://secret@example.com:443?type=grpc&serviceName=svc&sni=edge.example.com#trojan")
	if err != nil {
		t.Fatal(err)
	}
	if trojan.Extra["network"] != "grpc" || trojan.Extra["sni"] != "edge.example.com" {
		t.Fatalf("trojan extras = %#v", trojan.Extra)
	}

	hysteria, err := ParseURI("hy2://secret@example.com:443?obfs=salamander&sni=edge.example.com#hy2")
	if err != nil {
		t.Fatal(err)
	}
	if hysteria.Extra["obfs"] != nil || hysteria.Extra["udp"] != nil {
		t.Fatalf("hysteria2 extras = %#v", hysteria.Extra)
	}

	anyTLS, err := ParseURI("anytls://secret@example.com:443?type=ws&fp=chrome#anytls")
	if err != nil {
		t.Fatal(err)
	}
	if anyTLS.Extra["network"] != nil || anyTLS.Extra["client-fingerprint"] != "chrome" {
		t.Fatalf("anytls extras = %#v", anyTLS.Extra)
	}
}

func TestShadowsocksFallbackUsesLastAddressSeparators(t *testing.T) {
	payload := base64.RawURLEncoding.EncodeToString([]byte("aes-128-gcm:p@ss@[2001:db8::1]:443"))
	proxy, err := ParseURI("ss://" + payload + "#edge")
	if err != nil {
		t.Fatal(err)
	}
	if proxy.Server != "[2001:db8::1]" || proxy.Port != 443 || proxy.Extra["password"] != "p@ss" {
		t.Fatalf("proxy = %#v", proxy)
	}
}
