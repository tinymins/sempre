package layout

import (
	"os"
	"path/filepath"
	"runtime"
	"testing"
)

func TestAtCreatesPortableLayout(t *testing.T) {
	t.Parallel()
	root := t.TempDir()
	paths := At(root)
	if paths.Mode != Portable {
		t.Fatalf("mode = %q", paths.Mode)
	}
	if paths.Home != filepath.Join(root, ".sempre") {
		t.Fatalf("home = %q", paths.Home)
	}
	if paths.Logs != filepath.Join(paths.Home, "logs") || paths.Runtime != filepath.Join(paths.Home, "run") {
		t.Fatalf("layout = %#v", paths)
	}
}

func TestCorePathsIsolateRepositoriesWithTheSameVersion(t *testing.T) {
	t.Parallel()
	paths := At(t.TempDir())
	official := paths.CoreBinary("sing-box", "", "1.2.3")
	custom := paths.CoreBinary("sing-box", "tinymins/sing-box", "1.2.3")
	if official == custom {
		t.Fatal("default and custom repositories share a binary path")
	}
	wantCustom := filepath.Join(paths.Cores, "sing-box", "sources", "tinymins", "sing-box", "1.2.3", executableName("sing-box"))
	if custom != wantCustom {
		t.Fatalf("custom binary = %q, want %q", custom, wantCustom)
	}
}

func TestPortableAndSystemModesShareInstanceLock(t *testing.T) {
	portable, err := ForMode(Portable)
	if err != nil {
		t.Fatal(err)
	}
	system, err := ForMode(System)
	if err != nil {
		t.Fatal(err)
	}
	if portable.InstanceLock != system.InstanceLock {
		t.Fatalf("portable lock %q != system lock %q", portable.InstanceLock, system.InstanceLock)
	}
}

func TestPortableMarker(t *testing.T) {
	t.Parallel()
	executable := filepath.Join(t.TempDir(), executableName("sempre"))
	enabled, err := PortableMarkerEnabled(executable)
	if err != nil || enabled {
		t.Fatalf("initial marker = %v, %v", enabled, err)
	}
	if err := SetPortableMarker(executable, true); err != nil {
		t.Fatal(err)
	}
	enabled, err = PortableMarkerEnabled(executable)
	if err != nil || !enabled {
		t.Fatalf("enabled marker = %v, %v", enabled, err)
	}
	if err := SetPortableMarker(executable, false); err != nil {
		t.Fatal(err)
	}
	enabled, err = PortableMarkerEnabled(executable)
	if err != nil || enabled {
		t.Fatalf("disabled marker = %v, %v", enabled, err)
	}
}

func TestSystemLayoutUsesPlatformDirectories(t *testing.T) {
	t.Parallel()
	paths, err := ForMode(System)
	if err != nil {
		t.Fatal(err)
	}
	switch runtime.GOOS {
	case "windows":
		if filepath.Base(paths.Home) != "Sempre" || filepath.Base(paths.ServiceExecutable) != "sempre.exe" {
			t.Fatalf("layout = %#v", paths)
		}
	case "linux":
		if paths.Home != "/var/lib/sempre" || paths.Logs != "/var/log/sempre" || paths.Runtime != "/run/sempre" {
			t.Fatalf("layout = %#v", paths)
		}
	case "darwin":
		if paths.Home != "/Library/Application Support/Sempre/data" ||
			paths.Logs != "/Library/Logs/Sempre" ||
			paths.Runtime != "/var/run/sempre" ||
			paths.ServiceRoot != "/Library/Application Support/Sempre" {
			t.Fatalf("layout = %#v", paths)
		}
	}
}

func TestEnsureServiceExecutableDirectoryCreatesTraversableRoot(t *testing.T) {
	t.Parallel()
	base := t.TempDir()
	paths := SystemAt(base)
	paths.ServiceRoot = filepath.Join(base, "application")
	paths.ServiceExecutable = filepath.Join(paths.ServiceRoot, "bin", executableName("sempre"))
	paths.test = false
	if err := paths.EnsureServiceExecutableDirectory(); err != nil {
		t.Fatal(err)
	}
	for _, directory := range []string{paths.ServiceRoot, filepath.Dir(paths.ServiceExecutable)} {
		info, err := os.Stat(directory)
		if err != nil {
			t.Fatal(err)
		}
		if runtime.GOOS != "windows" && info.Mode().Perm() != 0o755 {
			t.Fatalf("%s mode = %o", directory, info.Mode().Perm())
		}
	}
}

func TestSystemAtKeepsRootsSeparate(t *testing.T) {
	t.Parallel()
	root := t.TempDir()
	paths := SystemAt(root)
	if paths.Mode != System {
		t.Fatalf("mode = %q", paths.Mode)
	}
	for _, path := range []string{paths.Home, paths.Logs, paths.Runtime, filepath.Dir(paths.ServiceExecutable), filepath.Dir(paths.CommandExecutable)} {
		if filepath.Dir(path) != root {
			t.Fatalf("%q is outside %q", path, root)
		}
	}
	if err := paths.Ensure(); err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(paths.Home); err != nil {
		t.Fatal(err)
	}
}
