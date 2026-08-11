package app

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"strings"
	"sync/atomic"
	"time"

	"github.com/tinymins/sempre/internal/control"
)

func (admin *adminServer) runtimeCapabilities(writer http.ResponseWriter, request *http.Request) {
	client, ok := admin.runtimeClient(writer)
	if !ok {
		return
	}
	apiWriteJSON(writer, http.StatusOK, client.Capabilities(request.Context()))
}

func (admin *adminServer) runtimeOverview(writer http.ResponseWriter, request *http.Request) {
	client, ok := admin.runtimeClient(writer)
	if !ok {
		return
	}
	result, err := client.Overview(request.Context())
	admin.writeRuntimeResult(writer, result, err)
}

func (admin *adminServer) runtimeConfig(writer http.ResponseWriter, request *http.Request) {
	client, ok := admin.runtimeClient(writer)
	if !ok {
		return
	}
	result, err := client.Config(request.Context())
	admin.writeRuntimeResult(writer, result, err)
}

func (admin *adminServer) runtimeConfigPatch(writer http.ResponseWriter, request *http.Request) {
	client, ok := admin.runtimeClient(writer)
	if !ok {
		return
	}
	var input map[string]any
	if !admin.decode(writer, request, &input) {
		return
	}
	err := client.PatchConfig(request.Context(), input)
	admin.writeRuntimeResult(writer, map[string]bool{"updated": err == nil, "persistent": false}, err)
}

func (admin *adminServer) runtimeProxies(writer http.ResponseWriter, request *http.Request) {
	client, ok := admin.runtimeClient(writer)
	if !ok {
		return
	}
	result, err := client.Proxies(request.Context())
	admin.writeRuntimeResult(writer, result, err)
}

func (admin *adminServer) runtimeProxySelect(writer http.ResponseWriter, request *http.Request) {
	client, ok := admin.runtimeClient(writer)
	if !ok {
		return
	}
	var input struct{ Group, Proxy string }
	if !admin.decode(writer, request, &input) {
		return
	}
	err := client.SelectProxy(request.Context(), input.Group, input.Proxy)
	admin.writeRuntimeResult(writer, map[string]bool{"selected": err == nil}, err)
}

func (admin *adminServer) runtimeProxyDelay(writer http.ResponseWriter, request *http.Request) {
	client, ok := admin.runtimeClient(writer)
	if !ok {
		return
	}
	var input struct {
		Name    string `json:"name"`
		URL     string `json:"url"`
		Timeout int    `json:"timeout"`
	}
	if !admin.decode(writer, request, &input) {
		return
	}
	if input.URL == "" {
		input.URL = "https://www.gstatic.com/generate_204"
	}
	if input.Timeout == 0 {
		input.Timeout = 5000
	}
	delay, err := client.ProxyDelay(request.Context(), input.Name, input.URL, input.Timeout)
	admin.writeRuntimeResult(writer, map[string]int{"delay": delay}, err)
}

func (admin *adminServer) runtimeProviders(writer http.ResponseWriter, request *http.Request) {
	client, ok := admin.runtimeClient(writer)
	if !ok {
		return
	}
	result, err := client.Providers(request.Context())
	admin.writeRuntimeResult(writer, result, err)
}

func (admin *adminServer) runtimeProviderUpdate(writer http.ResponseWriter, request *http.Request) {
	admin.runtimeProviderAction(writer, request, false)
}

func (admin *adminServer) runtimeProviderHealthcheck(writer http.ResponseWriter, request *http.Request) {
	admin.runtimeProviderAction(writer, request, true)
}

func (admin *adminServer) runtimeProviderAction(writer http.ResponseWriter, request *http.Request, healthcheck bool) {
	client, ok := admin.runtimeClient(writer)
	if !ok {
		return
	}
	var input struct {
		Name string `json:"name"`
	}
	if !admin.decode(writer, request, &input) {
		return
	}
	var err error
	if healthcheck {
		err = client.HealthcheckProvider(request.Context(), input.Name)
	} else {
		err = client.UpdateProvider(request.Context(), input.Name)
	}
	admin.writeRuntimeResult(writer, map[string]bool{"updated": err == nil}, err)
}

func (admin *adminServer) runtimeRules(writer http.ResponseWriter, request *http.Request) {
	client, ok := admin.runtimeClient(writer)
	if !ok {
		return
	}
	result, err := client.Rules(request.Context())
	admin.writeRuntimeResult(writer, result, err)
}

func (admin *adminServer) runtimeRuleProviders(writer http.ResponseWriter, request *http.Request) {
	client, ok := admin.runtimeClient(writer)
	if !ok {
		return
	}
	result, err := client.RuleProviders(request.Context())
	admin.writeRuntimeResult(writer, result, err)
}

func (admin *adminServer) runtimeRuleProviderUpdate(writer http.ResponseWriter, request *http.Request) {
	client, ok := admin.runtimeClient(writer)
	if !ok {
		return
	}
	var input struct {
		Name string `json:"name"`
	}
	if !admin.decode(writer, request, &input) {
		return
	}
	err := client.UpdateRuleProvider(request.Context(), input.Name)
	admin.writeRuntimeResult(writer, map[string]bool{"updated": err == nil}, err)
}

func (admin *adminServer) runtimeConnections(writer http.ResponseWriter, request *http.Request) {
	client, ok := admin.runtimeClient(writer)
	if !ok {
		return
	}
	result, err := client.Connections(request.Context())
	admin.writeRuntimeResult(writer, result, err)
}

func (admin *adminServer) runtimeConnectionClose(writer http.ResponseWriter, request *http.Request) {
	client, ok := admin.runtimeClient(writer)
	if !ok {
		return
	}
	var input struct {
		ID string `json:"id"`
	}
	if !admin.decode(writer, request, &input) {
		return
	}
	err := client.CloseConnection(request.Context(), input.ID)
	admin.writeRuntimeResult(writer, map[string]bool{"closed": err == nil}, err)
}

func (admin *adminServer) runtimeDNSQuery(writer http.ResponseWriter, request *http.Request) {
	client, ok := admin.runtimeClient(writer)
	if !ok {
		return
	}
	var input struct{ Name, Type string }
	if !admin.decode(writer, request, &input) {
		return
	}
	if input.Type == "" {
		input.Type = "A"
	}
	result, err := client.DNSQuery(request.Context(), input.Name, input.Type)
	admin.writeRuntimeResult(writer, result, err)
}

func (admin *adminServer) runtimeCacheFlush(writer http.ResponseWriter, request *http.Request) {
	client, ok := admin.runtimeClient(writer)
	if !ok {
		return
	}
	err := client.FlushFakeIP(request.Context())
	admin.writeRuntimeResult(writer, map[string]bool{"flushed": err == nil}, err)
}

func (admin *adminServer) runtimeStatus(writer http.ResponseWriter, _ *http.Request) {
	status, err := admin.manager.ManagedRuntimeStatus()
	if err != nil {
		admin.internalError(writer, err)
		return
	}
	apiWriteJSON(writer, http.StatusOK, status)
}

func (admin *adminServer) runtimeStart(writer http.ResponseWriter, _ *http.Request) {
	admin.runtimeAction(writer, RuntimeStart)
}

func (admin *adminServer) runtimeStop(writer http.ResponseWriter, _ *http.Request) {
	admin.runtimeAction(writer, RuntimeStop)
}

func (admin *adminServer) runtimeRestart(writer http.ResponseWriter, _ *http.Request) {
	admin.runtimeAction(writer, RuntimeRestart)
}

func (admin *adminServer) runtimeAction(writer http.ResponseWriter, action string) {
	status, err := admin.manager.ManagedRuntimeAction(action)
	if err != nil {
		var actionError *RuntimeActionError
		if errors.As(err, &actionError) {
			apiWriteError(writer, http.StatusConflict, actionError.Code, actionError.Message, map[string]any{"status": status})
			return
		}
		admin.internalError(writer, err)
		return
	}
	apiWriteJSON(writer, http.StatusAccepted, map[string]any{"action": action, "status": status})
}

func (admin *adminServer) runtimeReload(writer http.ResponseWriter, request *http.Request) {
	admin.manager.RequestReload()
	status, err := admin.manager.ManagedRuntimeStatus()
	if err != nil {
		admin.internalError(writer, err)
		return
	}
	apiWriteJSON(writer, http.StatusAccepted, map[string]any{"scheduled": true, "status": status})
}

type runtimeEvent struct {
	Topic string
	Data  json.RawMessage
	Error error
}

func (admin *adminServer) runtimeEvents(writer http.ResponseWriter, request *http.Request) {
	client, ok := admin.runtimeClient(writer)
	if !ok {
		return
	}
	flusher, ok := writer.(http.Flusher)
	if !ok {
		apiWriteError(writer, http.StatusInternalServerError, "STREAM_UNAVAILABLE", "streaming is unavailable", nil)
		return
	}
	allowed := map[string]bool{"traffic": true, "memory": true, "connections": true, "logs": true}
	seen := map[string]bool{}
	var topics []string
	for _, topic := range strings.Split(request.URL.Query().Get("topics"), ",") {
		topic = strings.TrimSpace(topic)
		if topic != "" && allowed[topic] && !seen[topic] {
			seen[topic] = true
			topics = append(topics, topic)
		}
	}
	if len(topics) == 0 {
		topics = []string{"traffic", "memory", "connections", "logs"}
	}
	writer.Header().Set("Content-Type", "text/event-stream")
	writer.Header().Set("Cache-Control", "no-cache")
	writer.Header().Set("X-Accel-Buffering", "no")
	writer.WriteHeader(http.StatusOK)
	flusher.Flush()
	ctx, cancel := context.WithCancel(request.Context())
	defer cancel()
	events := make(chan runtimeEvent, 64)
	for _, topic := range topics {
		go streamRuntimeTopic(ctx, client, topic, events)
	}
	heartbeat := time.NewTicker(15 * time.Second)
	defer heartbeat.Stop()
	var sequence atomic.Uint64
	for {
		select {
		case <-ctx.Done():
			return
		case <-heartbeat.C:
			_, _ = io.WriteString(writer, ": keepalive\n\n")
			flusher.Flush()
		case event := <-events:
			payload := map[string]any{
				"topic": event.Topic, "timestamp": time.Now().UTC(), "sequence": sequence.Add(1),
			}
			if event.Error != nil {
				payload["error"] = event.Error.Error()
			} else {
				payload["data"] = event.Data
			}
			data, _ := json.Marshal(payload)
			if _, err := fmt.Fprintf(writer, "event: %s\ndata: %s\n\n", event.Topic, data); err != nil {
				return
			}
			flusher.Flush()
		}
	}
}

func streamRuntimeTopic(ctx context.Context, client *control.Client, topic string, events chan<- runtimeEvent) {
	for ctx.Err() == nil {
		err := client.Stream(ctx, topic, func(data json.RawMessage) error {
			select {
			case events <- runtimeEvent{Topic: topic, Data: data}:
				return nil
			case <-ctx.Done():
				return ctx.Err()
			}
		})
		if ctx.Err() != nil {
			return
		}
		select {
		case events <- runtimeEvent{Topic: topic, Error: err}:
		case <-ctx.Done():
			return
		}
		timer := time.NewTimer(time.Second)
		select {
		case <-ctx.Done():
			timer.Stop()
			return
		case <-timer.C:
		}
	}
}
