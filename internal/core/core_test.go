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
		{"sing-box:tinymins/sing-box", Ref{Core: "sing-box", Repository: "tinymins/sing-box", Value: Stable}},
		{"sing-box:TinyMins/Sing-Box@v1.13.15-ddns.1", Ref{Core: "sing-box", Repository: "tinymins/sing-box", Value: "1.13.15-ddns.1"}},
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
			if reparsed, err := ParseRef(actual.String()); err != nil || reparsed != actual {
				t.Fatalf("ParseRef(%q) round trip = %#v, %v", actual.String(), reparsed, err)
			}
		})
	}
}

func TestParseRefRejectsInvalidValues(t *testing.T) {
	t.Parallel()
	for _, value := range []string{
		"", "SingBox", "sing-box@", "sing-box@latest", "sing-box@1.2", "../sing-box@1.2.3",
		"sing-box:", "sing-box:tinymins", "sing-box:https://github.com/tinymins/sing-box@1.2.3",
		"sing-box:tinymins/sing-box/extra@1.2.3", "sing-box:tinymins/sing-box@1.2.3@stable",
		"sing-box:tinymins/.@1.2.3", "sing-box:tinymins/..@1.2.3",
	} {
		if _, err := ParseRef(value); err == nil {
			t.Errorf("ParseRef(%q) succeeded", value)
		}
	}
}

func TestNormalizeAMD64Level(t *testing.T) {
	t.Parallel()
	for input, expected := range map[int]int{-1: 0, 0: 0, 1: 1, 2: 2, 3: 3, 4: 3} {
		if actual := normalizeAMD64Level(input); actual != expected {
			t.Fatalf("normalizeAMD64Level(%d) = %d, want %d", input, actual, expected)
		}
	}
}
