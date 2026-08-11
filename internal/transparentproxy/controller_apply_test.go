package transparentproxy

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
	"time"

	subscriptions "github.com/tinymins/sempre/internal/subscription"
)

func TestApplyTProxyRollsBackFailedVerification(t *testing.T) {
	first := listenTCP(t)
	defer first.Close()
	second := listenTCP(t)
	defer second.Close()
	backend := &fakeBackend{verifyTProxyErr: errors.New("missing rule")}
	controller := &Controller{backend: backend}
	plan := Plan{
		Mode:          subscriptions.TransparentProxyTProxy,
		TProxyPort:    listenerPort(t, first),
		DNSPort:       listenerPort(t, second),
		LANInterfaces: []string{"vmbr1"},
	}
	err := controller.Apply(context.Background(), plan)
	if err == nil {
		t.Fatal("expected verification failure")
	}
	if backend.applyCalls != 1 || backend.cleanupCalls != 1 {
		t.Fatalf("apply calls = %d, cleanup calls = %d", backend.applyCalls, backend.cleanupCalls)
	}
}

func TestSystemDNSTakeoverWritesAndRestoresResolvConf(t *testing.T) {
	stubSystemDNSChattr(t, nil)
	root := t.TempDir()
	resolv := filepath.Join(root, "resolv.conf")
	if err := os.WriteFile(resolv, []byte("nameserver 10.251.1.1\nnameserver 223.6.6.6\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	listener := listenTCP(t)
	defer listener.Close()
	controller := &Controller{
		backend:   &fakeBackend{},
		systemDNS: &systemDNSManager{allowed: true, stateDir: filepath.Join(root, "state"), resolvConf: resolv},
	}
	plan := Plan{SystemDNS: true, SystemDNSPort: listenerPort(t, listener)}
	if err := controller.Apply(context.Background(), plan); err != nil {
		t.Fatal(err)
	}
	current, err := os.ReadFile(resolv)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(current), "nameserver 127.0.0.1") {
		t.Fatalf("resolv.conf was not taken over: %q", current)
	}
	if err := controller.Cleanup(context.Background()); err != nil {
		t.Fatal(err)
	}
	current, err = os.ReadFile(resolv)
	if err != nil {
		t.Fatal(err)
	}
	if string(current) != "nameserver 10.251.1.1\nnameserver 223.6.6.6\n" {
		t.Fatalf("resolv.conf was not restored: %q", current)
	}
}

func TestSystemDNSTakeoverDoesNotOverwriteUserChangedResolvConf(t *testing.T) {
	stubSystemDNSChattr(t, nil)
	root := t.TempDir()
	resolv := filepath.Join(root, "resolv.conf")
	if err := os.WriteFile(resolv, []byte("nameserver 10.251.1.1\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	manager := &systemDNSManager{allowed: true, stateDir: filepath.Join(root, "state"), resolvConf: resolv}
	if err := manager.Apply(); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(resolv, []byte("nameserver 9.9.9.9\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := manager.Restore(); err != nil {
		t.Fatal(err)
	}
	current, err := os.ReadFile(resolv)
	if err != nil {
		t.Fatal(err)
	}
	if string(current) != "nameserver 9.9.9.9\n" {
		t.Fatalf("user resolv.conf change was overwritten: %q", current)
	}
}

func TestSystemDNSTakeoverLocksAndUnlocksResolvConf(t *testing.T) {
	var calls []bool
	stubSystemDNSChattr(t, func(_ string, immutable bool) error {
		calls = append(calls, immutable)
		return nil
	})
	root := t.TempDir()
	resolv := filepath.Join(root, "resolv.conf")
	if err := os.WriteFile(resolv, []byte("nameserver 10.251.1.1\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	manager := &systemDNSManager{allowed: true, stateDir: filepath.Join(root, "state"), resolvConf: resolv}
	if err := manager.Apply(); err != nil {
		t.Fatal(err)
	}
	if err := manager.Restore(); err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(calls, []bool{true, false}) {
		t.Fatalf("chattr calls = %#v", calls)
	}
}

func TestSystemDNSTakeoverIgnoresUnsupportedResolvConfLock(t *testing.T) {
	stubSystemDNSChattr(t, func(string, bool) error {
		return errors.New("operation not supported")
	})
	root := t.TempDir()
	resolv := filepath.Join(root, "resolv.conf")
	if err := os.WriteFile(resolv, []byte("nameserver 10.251.1.1\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	manager := &systemDNSManager{allowed: true, stateDir: filepath.Join(root, "state"), resolvConf: resolv}
	if err := manager.Apply(); err != nil {
		t.Fatal(err)
	}
	if err := manager.Restore(); err != nil {
		t.Fatal(err)
	}
}

func TestSystemDNSManagedRequiresFirstNameserver(t *testing.T) {
	if !systemDNSManaged([]byte("# comment\noptions timeout:1\nnameserver 127.0.0.1\nnameserver 10.251.1.1\n")) {
		t.Fatal("expected first nameserver 127.0.0.1 to be managed")
	}
	if systemDNSManaged([]byte("nameserver 10.251.1.1\nnameserver 127.0.0.1\n")) {
		t.Fatal("expected later 127.0.0.1 nameserver to be unmanaged")
	}
}

func stubSystemDNSChattr(t *testing.T, replacement func(string, bool) error) {
	t.Helper()
	previous := systemDNSChattr
	if replacement == nil {
		replacement = func(string, bool) error { return nil }
	}
	systemDNSChattr = replacement
	t.Cleanup(func() {
		systemDNSChattr = previous
	})
}

func TestApplyTUNReadinessTimeoutExplainsInterface(t *testing.T) {
	backend := &fakeBackend{verifyTUNErr: errors.New("Link not found")}
	controller := &Controller{backend: backend}
	oldTimeout := tunReadinessTimeout
	oldPollInterval := readinessPollInterval
	tunReadinessTimeout = 5 * time.Millisecond
	readinessPollInterval = time.Millisecond
	defer func() {
		tunReadinessTimeout = oldTimeout
		readinessPollInterval = oldPollInterval
	}()

	err := controller.Apply(context.Background(), Plan{Mode: subscriptions.TransparentProxyTUN, TUNInterface: "sempre-tun"})
	if err == nil {
		t.Fatal("expected TUN readiness timeout")
	}
	for _, part := range []string{"timed out waiting for TUN interface sempre-tun", "after 5ms", "Link not found"} {
		if !strings.Contains(err.Error(), part) {
			t.Fatalf("timeout error %q does not contain %q", err, part)
		}
	}
}
