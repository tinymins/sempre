package transparentproxy

import (
	"context"
	"errors"
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
