package main

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"net"
	"net/http"
	"os"
	"os/signal"
)

func main() {
	if len(os.Args) < 2 {
		fmt.Fprintln(os.Stderr, "missing command")
		os.Exit(2)
	}
	switch os.Args[1] {
	case "version":
		fmt.Println("sing-box version 1.2.3")
	case "check":
		if err := checkConfig(os.Args[2:]); err != nil {
			fmt.Fprintln(os.Stderr, err)
			os.Exit(2)
		}
	case "run":
		if err := checkConfig(os.Args[2:]); err != nil {
			fmt.Fprintln(os.Stderr, err)
			os.Exit(2)
		}
		ctx, cancel := signal.NotifyContext(context.Background(), handledSignals()...)
		defer cancel()
		if err := runCore(ctx, os.Args[2:]); err != nil {
			fmt.Fprintln(os.Stderr, err)
			os.Exit(2)
		}
	default:
		fmt.Fprintln(os.Stderr, "unsupported command")
		os.Exit(2)
	}
}

func checkConfig(arguments []string) error {
	path, err := configurationPath(arguments)
	if err != nil {
		return err
	}
	info, err := os.Stat(path)
	if err != nil {
		return fmt.Errorf("inspect configuration: %w", err)
	}
	if info.IsDir() {
		return fmt.Errorf("configuration is a directory")
	}
	return nil
}

func configurationPath(arguments []string) (string, error) {
	for index := 0; index+1 < len(arguments); index++ {
		if arguments[index] != "-c" {
			continue
		}
		return arguments[index+1], nil
	}
	return "", fmt.Errorf("configuration argument is missing")
}

func runCore(ctx context.Context, arguments []string) error {
	path, err := configurationPath(arguments)
	if err != nil {
		return err
	}
	data, err := os.ReadFile(path)
	if err != nil {
		return err
	}
	var config struct {
		Experimental struct {
			ClashAPI struct {
				ExternalController string `json:"external_controller"`
				Secret             string `json:"secret"`
			} `json:"clash_api"`
		} `json:"experimental"`
	}
	if err := json.Unmarshal(data, &config); err != nil {
		return fmt.Errorf("decode configuration: %w", err)
	}
	listener, err := net.Listen("tcp", config.Experimental.ClashAPI.ExternalController)
	if err != nil {
		return fmt.Errorf("listen on control API: %w", err)
	}
	server := &http.Server{Handler: controlHandler(config.Experimental.ClashAPI.Secret)}
	go func() {
		<-ctx.Done()
		_ = server.Shutdown(context.Background())
	}()
	fmt.Println("test core started")
	err = server.Serve(listener)
	if errors.Is(err, http.ErrServerClosed) {
		return nil
	}
	return err
}

func controlHandler(secret string) http.Handler {
	return http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		if request.Header.Get("Authorization") != "Bearer "+secret {
			writer.WriteHeader(http.StatusUnauthorized)
			return
		}
		if request.Method != http.MethodGet || request.URL.Path != "/version" {
			http.NotFound(writer, request)
			return
		}
		writer.Header().Set("Content-Type", "application/json")
		_, _ = writer.Write([]byte(`{"version":"1.2.3","meta":false}`))
	})
}
