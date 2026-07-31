package core

import "testing"

func TestParseRef(t *testing.T) {
	t.Parallel()
	tests := []struct {
		input string
		want  Ref
	}{
		{"sing-box", Ref{Core: "sing-box", Value: Stable}},
		{"sing-box@stable", Ref{Core: "sing-box", Value: Stable}},
		{"sing-box@1.13.15", Ref{Core: "sing-box", Value: "1.13.15"}},
		{"sing-box@v1.14.0-alpha.1", Ref{Core: "sing-box", Value: "1.14.0-alpha.1"}},
	}
	for _, test := range tests {
		test := test
		t.Run(test.input, func(t *testing.T) {
			t.Parallel()
			actual, err := ParseRef(test.input)
			if err != nil {
				t.Fatal(err)
			}
			if actual != test.want {
				t.Fatalf("ParseRef(%q) = %#v, want %#v", test.input, actual, test.want)
			}
		})
	}
}

func TestParseRefRejectsInvalidValues(t *testing.T) {
	t.Parallel()
	for _, value := range []string{"", "SingBox", "sing-box@", "sing-box@latest", "sing-box@1.2", "../sing-box@1.2.3"} {
		if _, err := ParseRef(value); err == nil {
			t.Errorf("ParseRef(%q) succeeded", value)
		}
	}
}
