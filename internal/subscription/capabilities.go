package subscription

import (
	"crypto/sha256"
	"encoding/hex"

	"github.com/tinymins/sempre/internal/core"
)

type ConfigurationTarget struct {
	Core           string `json:"core"`
	Version        string `json:"version"`
	CompilerTarget Target `json:"compiler_target"`
	Key            string `json:"key"`
}

type RunningCore struct {
	Core    string `json:"core"`
	Version string `json:"version"`
}

type ConfigurationContext struct {
	Key          string               `json:"key"`
	Target       *ConfigurationTarget `json:"target,omitempty"`
	Running      *RunningCore         `json:"running,omitempty"`
	Platform     string               `json:"platform"`
	Capabilities core.Capabilities    `json:"capabilities"`
}

func NewConfigurationTarget(coreID, version string, target Target) ConfigurationTarget {
	keyData := coreID + "|" + version + "|" + target.Format + "|" + target.Platform
	sum := sha256.Sum256([]byte(keyData))
	return ConfigurationTarget{
		Core: coreID, Version: version, CompilerTarget: target,
		Key: hex.EncodeToString(sum[:]),
	}
}
