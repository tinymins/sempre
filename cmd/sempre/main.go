package main

import (
	"context"
	"os"
	"os/signal"

	"github.com/sempre-lab/sempre/internal/cli"
)

func main() {
	ctx, cancel := signal.NotifyContext(context.Background(), handledSignals()...)
	defer cancel()
	os.Exit(cli.Run(ctx, os.Args[1:], os.Stdin, os.Stdout, os.Stderr))
}
