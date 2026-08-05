package main

import (
	"context"
	"fmt"
	"os"
	"os/signal"
	"path/filepath"

	"github.com/tinymins/sempre/internal/app"
	"github.com/tinymins/sempre/internal/layout"
)

const developmentListen = "127.0.0.1:33212"

func main() {
	ctx, cancel := signal.NotifyContext(context.Background(), handledSignals()...)
	defer cancel()

	paths := layout.At(filepath.Join(".cache", "sempre-dev", "runtime"))
	manager, err := app.NewDevelopment(paths, os.Stdout, os.Stderr)
	if err != nil {
		fail(err)
	}
	status, err := manager.SetWebListen(developmentListen)
	if err != nil {
		fail(err)
	}
	fmt.Fprintln(os.Stdout, "Sempre development API:", status.LocalURL)
	if err := manager.RunDaemon(ctx); err != nil {
		fail(err)
	}
}

func fail(err error) {
	fmt.Fprintln(os.Stderr, "ERROR:", err)
	os.Exit(1)
}
