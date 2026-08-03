package webconfig

import (
	"crypto/rand"
	"crypto/subtle"
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"net"
	"net/url"
	"os"
	"strconv"
	"strings"
	"sync"
	"time"

	"golang.org/x/crypto/argon2"

	"github.com/tinymins/sempre/internal/state"
)

const (
	SchemaVersion = 1
	DefaultListen = "127.0.0.1:33211"

	argonMemory      = 64 * 1024
	argonIterations  = 3
	argonParallelism = 2
	argonKeyLength   = 32
)

type Config struct {
	Schema   int    `json:"schema"`
	Listen   string `json:"listen"`
	Password string `json:"password,omitempty"`
}

type Endpoint struct {
	Schema    int       `json:"schema"`
	APIMajor  int       `json:"api_major"`
	Version   string    `json:"version"`
	Bind      string    `json:"bind"`
	LocalURL  string    `json:"local_url"`
	UpdatedAt time.Time `json:"updated_at"`
}

type Store struct {
	path string
	mu   sync.Mutex
}

func New(path string) *Store {
	return &Store{path: path}
}

func (store *Store) Initialize() error {
	store.mu.Lock()
	defer store.mu.Unlock()
	_, err := store.readUnlocked()
	return err
}

func (store *Store) Read() (Config, error) {
	store.mu.Lock()
	defer store.mu.Unlock()
	return store.readUnlocked()
}

func (store *Store) Update(change func(*Config) error) (Config, error) {
	store.mu.Lock()
	defer store.mu.Unlock()
	config, err := store.readUnlocked()
	if err != nil {
		return Config{}, err
	}
	if err := change(&config); err != nil {
		return Config{}, err
	}
	config.Schema = SchemaVersion
	if err := config.Validate(); err != nil {
		return Config{}, err
	}
	if err := write(store.path, config, 0o600); err != nil {
		return Config{}, err
	}
	return config, nil
}

func (store *Store) SetPassword(password string) (Config, error) {
	hash := ""
	var err error
	if password != "" {
		hash, err = HashPassword(password)
		if err != nil {
			return Config{}, err
		}
	}
	return store.Update(func(config *Config) error {
		config.Password = hash
		return nil
	})
}

func (config Config) Validate() error {
	if config.Schema != SchemaVersion {
		return fmt.Errorf("unsupported web configuration schema %d", config.Schema)
	}
	if err := ValidateListen(config.Listen); err != nil {
		return err
	}
	if config.Password != "" {
		if _, err := parsePasswordHash(config.Password); err != nil {
			return fmt.Errorf("invalid administrator password record: %w", err)
		}
	}
	return nil
}

func ValidateListen(value string) error {
	if strings.TrimSpace(value) != value || value == "" {
		return fmt.Errorf("listen address cannot be empty or contain surrounding whitespace")
	}
	host, portText, err := net.SplitHostPort(value)
	if err != nil {
		return fmt.Errorf("listen address must be host:port: %w", err)
	}
	if host == "" {
		return fmt.Errorf("listen host cannot be empty; use 0.0.0.0 or :: for all interfaces")
	}
	port, err := strconv.Atoi(portText)
	if err != nil || port < 1 || port > 65535 {
		return fmt.Errorf("listen port must be between 1 and 65535")
	}
	return nil
}

func LocalURL(listen string) (string, error) {
	host, port, err := net.SplitHostPort(listen)
	if err != nil {
		return "", err
	}
	switch host {
	case "0.0.0.0":
		host = "127.0.0.1"
	case "::", "[::]":
		host = "::1"
	}
	return (&url.URL{Scheme: "http", Host: net.JoinHostPort(host, port)}).String(), nil
}

func WriteEndpoint(path string, endpoint Endpoint) error {
	endpoint.Schema = SchemaVersion
	endpoint.APIMajor = 1
	endpoint.UpdatedAt = time.Now().UTC()
	return write(path, endpoint, 0o644)
}

func ReadEndpoint(path string) (Endpoint, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return Endpoint{}, err
	}
	var endpoint Endpoint
	if err := json.Unmarshal(data, &endpoint); err != nil {
		return Endpoint{}, fmt.Errorf("decode endpoint discovery: %w", err)
	}
	if endpoint.Schema != SchemaVersion || endpoint.APIMajor != 1 || endpoint.LocalURL == "" {
		return Endpoint{}, fmt.Errorf("endpoint discovery is incompatible")
	}
	return endpoint, nil
}

func HashPassword(password string) (string, error) {
	if password == "" {
		return "", nil
	}
	salt := make([]byte, 16)
	if _, err := rand.Read(salt); err != nil {
		return "", fmt.Errorf("generate password salt: %w", err)
	}
	key := argon2.IDKey([]byte(password), salt, argonIterations, argonMemory, argonParallelism, argonKeyLength)
	return fmt.Sprintf(
		"$argon2id$v=19$m=%d,t=%d,p=%d$%s$%s",
		argonMemory,
		argonIterations,
		argonParallelism,
		base64.RawStdEncoding.EncodeToString(salt),
		base64.RawStdEncoding.EncodeToString(key),
	), nil
}

func VerifyPassword(encoded, password string) bool {
	if encoded == "" {
		return true
	}
	parameters, err := parsePasswordHash(encoded)
	if err != nil {
		return false
	}
	actual := argon2.IDKey(
		[]byte(password),
		parameters.salt,
		parameters.iterations,
		parameters.memory,
		parameters.parallelism,
		uint32(len(parameters.key)),
	)
	return subtle.ConstantTimeCompare(actual, parameters.key) == 1
}

type passwordParameters struct {
	memory      uint32
	iterations  uint32
	parallelism uint8
	salt        []byte
	key         []byte
}

func parsePasswordHash(encoded string) (passwordParameters, error) {
	parts := strings.Split(encoded, "$")
	if len(parts) != 6 || parts[1] != "argon2id" || parts[2] != "v=19" {
		return passwordParameters{}, fmt.Errorf("unsupported password hash")
	}
	var parameters passwordParameters
	if _, err := fmt.Sscanf(parts[3], "m=%d,t=%d,p=%d", &parameters.memory, &parameters.iterations, &parameters.parallelism); err != nil {
		return passwordParameters{}, fmt.Errorf("decode password parameters: %w", err)
	}
	if parameters.memory < 8*1024 || parameters.iterations < 1 || parameters.parallelism < 1 {
		return passwordParameters{}, fmt.Errorf("unsafe password parameters")
	}
	var err error
	parameters.salt, err = base64.RawStdEncoding.DecodeString(parts[4])
	if err != nil || len(parameters.salt) < 8 {
		return passwordParameters{}, fmt.Errorf("invalid password salt")
	}
	parameters.key, err = base64.RawStdEncoding.DecodeString(parts[5])
	if err != nil || len(parameters.key) < 16 {
		return passwordParameters{}, fmt.Errorf("invalid password key")
	}
	return parameters, nil
}

func (store *Store) readUnlocked() (Config, error) {
	data, err := os.ReadFile(store.path)
	if errors.Is(err, os.ErrNotExist) {
		config := Config{Schema: SchemaVersion, Listen: DefaultListen}
		if err := write(store.path, config, 0o600); err != nil {
			return Config{}, err
		}
		return config, nil
	}
	if err != nil {
		return Config{}, fmt.Errorf("read web configuration: %w", err)
	}
	var config Config
	if err := json.Unmarshal(data, &config); err != nil {
		return Config{}, fmt.Errorf("decode web configuration: %w", err)
	}
	if err := config.Validate(); err != nil {
		return Config{}, err
	}
	return config, nil
}

func write(path string, value any, mode os.FileMode) error {
	data, err := json.MarshalIndent(value, "", "  ")
	if err != nil {
		return err
	}
	return state.WriteAtomic(path, append(data, '\n'), mode)
}
