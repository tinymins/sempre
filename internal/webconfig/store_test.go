package webconfig

import (
	"path/filepath"
	"testing"
)

func TestStoreInitializesWithLoopbackDefault(t *testing.T) {
	t.Parallel()
	store := New(filepath.Join(t.TempDir(), "web.json"))
	if err := store.Initialize(); err != nil {
		t.Fatal(err)
	}
	config, err := store.Read()
	if err != nil {
		t.Fatal(err)
	}
	if config.Schema != SchemaVersion || config.Listen != DefaultListen || config.Password != "" {
		t.Fatalf("default config = %#v", config)
	}
}

func TestPasswordHashRoundTrip(t *testing.T) {
	hash, err := HashPassword("correct horse battery staple")
	if err != nil {
		t.Fatal(err)
	}
	if hash == "correct horse battery staple" || !VerifyPassword(hash, "correct horse battery staple") {
		t.Fatal("password hash did not verify")
	}
	if VerifyPassword(hash, "wrong") || VerifyPassword("invalid", "wrong") {
		t.Fatal("invalid password was accepted")
	}
}

func TestListenValidationAndLocalURL(t *testing.T) {
	t.Parallel()
	for _, value := range []string{"", "localhost", "127.0.0.1:0", ":33211", " 127.0.0.1:33211"} {
		if err := ValidateListen(value); err == nil {
			t.Errorf("accepted invalid listen address %q", value)
		}
	}
	for input, expected := range map[string]string{
		"127.0.0.1:33211": "http://127.0.0.1:33211",
		"0.0.0.0:33211":   "http://127.0.0.1:33211",
		"[::]:33211":      "http://[::1]:33211",
	} {
		if err := ValidateListen(input); err != nil {
			t.Fatalf("validate %q: %v", input, err)
		}
		actual, err := LocalURL(input)
		if err != nil || actual != expected {
			t.Errorf("LocalURL(%q) = %q, %v; want %q", input, actual, err, expected)
		}
	}
}
