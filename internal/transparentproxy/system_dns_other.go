//go:build !linux

package transparentproxy

type systemDNSManager struct {
	allowed    bool
	stateDir   string
	resolvConf string
}

func (manager *systemDNSManager) Apply() error   { return nil }
func (manager *systemDNSManager) Restore() error { return nil }
func (manager *systemDNSManager) Verify() error  { return nil }
