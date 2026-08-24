package main

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"flag"
	"fmt"
	"os"
	"path/filepath"
	"time"

	"github.com/tinymins/sempre/internal/layout"
	"github.com/tinymins/sempre/internal/state"
	subscriptions "github.com/tinymins/sempre/internal/subscription"
	uiassets "github.com/tinymins/sempre/internal/ui"
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
	if err := state.WriteAtomic(paths.CoreBinary("sing-box", "", "1.2.3"), coreData, 0o755); err != nil {
		return err
	}
	configData := []byte("{\"log\":{\"disabled\":true},\"inbounds\":[],\"outbounds\":[]}")
	digest := sha256.Sum256(configData)
	hash := hex.EncodeToString(digest[:])
	if err := state.WriteAtomic(paths.Config("sing-box", hash), configData, 0o600); err != nil {
		return err
	}
	if err := setupUI(paths); err != nil {
		return err
	}
	profileID, err := setupSubscription(paths)
	if err != nil {
		return err
	}
	return store.Update(func(document *state.Document) error {
		source := document.Core("sing-box").Source("")
		source.Channels["stable"] = "1.2.3"
		source.Installed["1.2.3"] = &state.Installation{
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
		document.ActiveProfileID = profileID
		document.Subscription.Interval = "off"
		document.Runtime = state.Runtime{}
		return nil
	})
}

func setupSubscription(paths layout.Layout) (string, error) {
	store := subscriptions.NewStore(paths)
	if err := store.Initialize(""); err != nil {
		return "", err
	}
	catalog, err := store.Read()
	if err != nil {
		return "", err
	}
	profileID := catalog.Profiles[0].ID
	err = store.Update(func(candidate *subscriptions.Catalog) error {
		profile, findErr := subscriptions.FindProfile(candidate, profileID)
		if findErr != nil {
			return findErr
		}
		profile.TransparentProxy.Mode = subscriptions.TransparentProxyDisabled
		return nil
	})
	return profileID, err
}

func setupUI(paths layout.Layout) error {
	manifest := uiassets.Manifest{Schema: 1, Name: "Sempre Integration UI", Version: "1.0.0", Entry: "index.html", API: uiassets.API{Major: 1}}
	digest := sha256.Sum256([]byte("sempre integration UI"))
	metadata := uiassets.Metadata{
		Manifest: manifest, SourceType: "local", Source: "integration-test",
		Digest: hex.EncodeToString(digest[:]), InstalledAt: time.Now().UTC(),
	}
	manifestData, err := json.Marshal(manifest)
	if err != nil {
		return err
	}
	metadataData, err := json.Marshal(metadata)
	if err != nil {
		return err
	}
	if err := os.MkdirAll(paths.UICurrent, 0o700); err != nil {
		return err
	}
	for name, data := range map[string][]byte{
		"index.html":          []byte("<!doctype html><title>Sempre Integration UI</title>"),
		uiassets.ManifestName: manifestData,
		uiassets.MetadataName: metadataData,
	} {
		if err := state.WriteAtomic(filepath.Join(paths.UICurrent, name), data, 0o600); err != nil {
			return err
		}
	}
	return nil
}
