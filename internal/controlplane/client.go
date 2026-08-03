package controlplane

import (
	"bytes"
	"context"
	"crypto/rand"
	"crypto/subtle"
	"encoding/base64"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"os"
	"strings"
	"time"

	"github.com/tinymins/sempre/internal/state"
)

const (
	SchemaVersion = 1
	APIMajor      = 1
	TokenHeader   = "X-Sempre-Daemon-Token"
)

type Endpoint struct {
	Schema    int       `json:"schema"`
	APIMajor  int       `json:"api_major"`
	BaseURL   string    `json:"base_url"`
	Token     string    `json:"token"`
	UpdatedAt time.Time `json:"updated_at"`
}

type Client struct {
	endpoint Endpoint
	http     *http.Client
}

type HTTPError struct {
	Status  int
	Code    string
	Message string
	Details json.RawMessage
}

func (failure *HTTPError) Error() string {
	if failure.Code == "" {
		return failure.Message
	}
	return fmt.Sprintf("%s: %s", failure.Code, failure.Message)
}

func NewToken() (string, error) {
	data := make([]byte, 32)
	if _, err := rand.Read(data); err != nil {
		return "", fmt.Errorf("generate daemon control token: %w", err)
	}
	return base64.RawURLEncoding.EncodeToString(data), nil
}

func WriteEndpoint(path, baseURL, token string) error {
	if baseURL == "" || token == "" {
		return fmt.Errorf("daemon control endpoint requires a URL and token")
	}
	data, err := json.MarshalIndent(Endpoint{
		Schema:    SchemaVersion,
		APIMajor:  APIMajor,
		BaseURL:   strings.TrimRight(baseURL, "/"),
		Token:     token,
		UpdatedAt: time.Now().UTC(),
	}, "", "  ")
	if err != nil {
		return err
	}
	return state.WriteAtomic(path, append(data, '\n'), 0o600)
}

func ReadEndpoint(path string) (Endpoint, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return Endpoint{}, fmt.Errorf("read daemon control endpoint: %w", err)
	}
	var endpoint Endpoint
	if err := json.Unmarshal(data, &endpoint); err != nil {
		return Endpoint{}, fmt.Errorf("decode daemon control endpoint: %w", err)
	}
	if endpoint.Schema != SchemaVersion || endpoint.APIMajor != APIMajor || endpoint.BaseURL == "" || endpoint.Token == "" {
		return Endpoint{}, fmt.Errorf("daemon control endpoint is incompatible")
	}
	return endpoint, nil
}

func Discover(path string) (*Client, error) {
	endpoint, err := ReadEndpoint(path)
	if err != nil {
		return nil, fmt.Errorf("Sempre daemon is unavailable: %w", err)
	}
	return &Client{endpoint: endpoint, http: &http.Client{Timeout: 65 * time.Second}}, nil
}

func (client *Client) Get(ctx context.Context, path string, output any) error {
	return client.do(ctx, http.MethodGet, path, nil, output)
}

func (client *Client) Post(ctx context.Context, path string, input, output any) error {
	return client.do(ctx, http.MethodPost, path, input, output)
}

func (client *Client) do(ctx context.Context, method, path string, input, output any) error {
	var body io.Reader
	if input != nil {
		data, err := json.Marshal(input)
		if err != nil {
			return err
		}
		body = bytes.NewReader(data)
	}
	request, err := http.NewRequestWithContext(ctx, method, client.endpoint.BaseURL+path, body)
	if err != nil {
		return err
	}
	request.Header.Set("Accept", "application/json")
	request.Header.Set(TokenHeader, client.endpoint.Token)
	if input != nil {
		request.Header.Set("Content-Type", "application/json")
	}
	response, err := client.http.Do(request)
	if err != nil {
		return fmt.Errorf("contact Sempre daemon: %w", err)
	}
	defer response.Body.Close()
	data, err := io.ReadAll(io.LimitReader(response.Body, 2<<20))
	if err != nil {
		return err
	}
	if response.StatusCode < 200 || response.StatusCode >= 300 {
		var envelope struct {
			Error struct {
				Code    string          `json:"code"`
				Message string          `json:"message"`
				Details json.RawMessage `json:"details"`
			} `json:"error"`
		}
		_ = json.Unmarshal(data, &envelope)
		message := envelope.Error.Message
		if message == "" {
			message = http.StatusText(response.StatusCode)
		}
		return &HTTPError{Status: response.StatusCode, Code: envelope.Error.Code, Message: message, Details: envelope.Error.Details}
	}
	if output == nil || response.StatusCode == http.StatusNoContent {
		return nil
	}
	if err := json.Unmarshal(data, output); err != nil {
		return fmt.Errorf("decode Sempre daemon response: %w", err)
	}
	return nil
}

func EqualToken(left, right string) bool {
	if left == "" || len(left) != len(right) {
		return false
	}
	return subtle.ConstantTimeCompare([]byte(left), []byte(right)) == 1
}
