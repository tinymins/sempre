//go:build !linux

package transparentproxy

import (
	"context"
	"fmt"
)

type unsupportedBackend struct{}

func newSystemBackend() systemBackend {
	return unsupportedBackend{}
}

func (unsupportedBackend) Supported() bool { return false }

func (unsupportedBackend) Inventory(context.Context) (Inventory, error) { return Inventory{}, nil }

func (unsupportedBackend) RequirePrivileges() error {
	return fmt.Errorf("Linux transparent proxy mode is unavailable on this platform")
}

func (unsupportedBackend) IPv4Forwarding() (bool, error) { return false, nil }

func (unsupportedBackend) ApplyTProxy(context.Context, Plan) error { return nil }

func (unsupportedBackend) VerifyTProxy(context.Context, Plan) error { return nil }

func (unsupportedBackend) VerifyTUN(context.Context, Plan) error { return nil }

func (unsupportedBackend) Cleanup(context.Context) error { return nil }
