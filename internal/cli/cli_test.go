package cli

import "testing"

func TestParseGlobalOptions(t *testing.T) {
	t.Parallel()
	arguments, options, err := parseGlobalOptions([]string{"subscription", "update", "--yes", "--elevated"})
	if err != nil {
		t.Fatal(err)
	}
	if !options.Yes || options.NoRestart {
		t.Fatalf("options = %#v", options)
	}
	if len(arguments) != 2 || arguments[0] != "subscription" || arguments[1] != "update" {
		t.Fatalf("arguments = %#v", arguments)
	}
}

func TestParseGlobalOptionsRejectsConflictingRestartFlags(t *testing.T) {
	t.Parallel()
	if _, _, err := parseGlobalOptions([]string{"update", "--yes", "--no-restart"}); err == nil {
		t.Fatal("conflicting restart flags were accepted")
	}
}
