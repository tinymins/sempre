package main

import (
	"crypto/sha256"
	"encoding/hex"
	"flag"
	"fmt"
	"os"
	"time"

	"github.com/tinymins/sempre/internal/layout"
	"github.com/tinymins/sempre/internal/state"
)

func main() {
	root := flag.String("root", "", "directory containing the Sempre binary")
	coreBinary := flag.String("core", "", "path to the test core binary")
	flag.Parse()
	if *root == "" || *coreBinary == "" {
		fmt.Fprintln(os.Stderr, "-root and -core are required")
		os.Exit(2)
	}
	if err := setup(*root, *coreBinary); err != nil {
		fmt.Fprintln(os.Stderr, "prepare smoke deployment:", err)
		os.Exit(1)
	}
}

func setup(root, coreBinary string) error {
	paths := layout.At(root)
	store := state.New(paths)
	if err := store.Initialize(); err != nil {
		return err
	}
	coreData, err := os.ReadFile(coreBinary)
	if err != nil {
		return err
	}
	if err := state.WriteAtomic(paths.CoreBinary("sing-box", "1.2.3"), coreData, 0o755); err != nil {
		return err
	}
	configData := []byte("{\"log\":{\"disabled\":true},\"inbounds\":[],\"outbounds\":[]}")
	digest := sha256.Sum256(configData)
	hash := hex.EncodeToString(digest[:])
	if err := state.WriteAtomic(paths.Config("sing-box", hash), configData, 0o600); err != nil {
		return err
	}
	return store.Update(func(document *state.Document) error {
		coreState := document.Core("sing-box")
		coreState.Channels["stable"] = "1.2.3"
		coreState.Installed["1.2.3"] = &state.Installation{
			Explicit:    true,
			Digest:      "sha256:integration-test",
			Source:      "integration-test",
			InstalledAt: time.Now().UTC(),
		}
		document.Selected = &state.Selection{Core: "sing-box", Ref: "stable"}
		document.Configs["sing-box"] = hash
		document.Active = &state.Deployment{
			Core:       "sing-box",
			Ref:        "stable",
			Version:    "1.2.3",
			ConfigHash: hash,
		}
		document.Previous = nil
		document.Pending = false
		document.Subscription.Interval = "off"
		document.Runtime = state.Runtime{}
		return nil
	})
}
