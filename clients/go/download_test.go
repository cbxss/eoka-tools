package eoka

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"runtime"
	"testing"
)

func TestAssetSuffixFor(t *testing.T) {
	cases := []struct {
		goos, goarch string
		want         string
		wantErr      bool
	}{
		{"linux", "amd64", "linux-amd64", false},
		{"linux", "arm64", "linux-arm64", false},
		{"darwin", "amd64", "darwin-amd64", false},
		{"darwin", "arm64", "darwin-arm64", false},
		{"windows", "amd64", "windows-amd64.exe", false},
		{"windows", "arm64", "", true},
		{"freebsd", "amd64", "", true},
	}
	for _, c := range cases {
		got, err := assetSuffixFor(c.goos, c.goarch)
		if c.wantErr {
			if err == nil {
				t.Errorf("assetSuffixFor(%q, %q): expected error, got %q", c.goos, c.goarch, got)
			}
			continue
		}
		if err != nil || got != c.want {
			t.Errorf("assetSuffixFor(%q, %q) = %q, %v; want %q, nil", c.goos, c.goarch, got, err, c.want)
		}
	}
}

func TestChecksumFor(t *testing.T) {
	data := []byte("abc123  eoka-server-linux-amd64\ndef456  eoka-server-darwin-arm64\n")
	got, err := checksumFor(data, "eoka-server-darwin-arm64")
	if err != nil || got != "def456" {
		t.Fatalf("checksumFor: %q, %v", got, err)
	}
	if _, err := checksumFor(data, "eoka-server-windows-amd64.exe"); err == nil {
		t.Fatal("expected error for missing asset")
	}
}

func newFakeReleaseServer(t *testing.T, assetName, assetBody string) (*httptest.Server, *int) {
	t.Helper()
	sum := sha256.Sum256([]byte(assetBody))
	checksums := fmt.Sprintf("%s  %s\n", hex.EncodeToString(sum[:]), assetName)

	hits := 0
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		hits++
		switch filepath.Base(r.URL.Path) {
		case "checksums.txt":
			_, _ = w.Write([]byte(checksums))
		case assetName:
			_, _ = w.Write([]byte(assetBody))
		default:
			http.NotFound(w, r)
		}
	}))
	t.Cleanup(srv.Close)
	return srv, &hits
}

func withFakeReleaseServer(t *testing.T, srv *httptest.Server) {
	t.Helper()
	prev := releaseBaseURL
	releaseBaseURL = srv.URL
	t.Cleanup(func() { releaseBaseURL = prev })
}

func withCacheDir(t *testing.T, path string) {
	t.Helper()
	previous := userCacheDir
	userCacheDir = func() (string, error) { return path, nil }
	t.Cleanup(func() { userCacheDir = previous })
}

func TestEnsureServerBinaryDownloadsAndCaches(t *testing.T) {
	suffix, err := assetSuffixFor(runtime.GOOS, runtime.GOARCH)
	if err != nil {
		t.Skipf("no prebuilt eoka-server binary for %s/%s", runtime.GOOS, runtime.GOARCH)
	}
	assetName := "eoka-server-" + suffix
	body := "#!/bin/sh\necho fake-eoka-server\n"

	srv, hits := newFakeReleaseServer(t, assetName, body)
	withFakeReleaseServer(t, srv)
	withCacheDir(t, t.TempDir())

	ctx := context.Background()

	path, err := ensureServerBinary(ctx)
	if err != nil {
		t.Fatalf("ensureServerBinary: %v", err)
	}
	got, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("reading downloaded binary: %v", err)
	}
	if string(got) != body {
		t.Fatalf("downloaded binary content mismatch: got %q, want %q", got, body)
	}
	info, err := os.Stat(path)
	if err != nil {
		t.Fatalf("stat: %v", err)
	}
	if info.Mode().Perm()&0o100 == 0 {
		t.Fatalf("downloaded binary is not executable: mode %v", info.Mode())
	}
	if *hits != 2 {
		t.Fatalf("expected 2 requests (checksums.txt + asset), got %d", *hits)
	}

	path2, err := ensureServerBinary(ctx)
	if err != nil {
		t.Fatalf("ensureServerBinary (cached): %v", err)
	}
	if path2 != path {
		t.Fatalf("cached path mismatch: %q != %q", path2, path)
	}
	if *hits != 2 {
		t.Fatalf("expected no additional requests on cache hit, got %d total", *hits)
	}
}

func TestEnsureServerBinaryChecksumMismatch(t *testing.T) {
	suffix, err := assetSuffixFor(runtime.GOOS, runtime.GOARCH)
	if err != nil {
		t.Skipf("no prebuilt eoka-server binary for %s/%s", runtime.GOOS, runtime.GOARCH)
	}
	assetName := "eoka-server-" + suffix
	wrongSum := "0000000000000000000000000000000000000000000000000000000000000000"[:64]

	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch filepath.Base(r.URL.Path) {
		case "checksums.txt":
			_, _ = w.Write([]byte(wrongSum + "  " + assetName + "\n"))
		case assetName:
			_, _ = w.Write([]byte("not what the checksum says"))
		default:
			http.NotFound(w, r)
		}
	}))
	t.Cleanup(srv.Close)
	withFakeReleaseServer(t, srv)
	withCacheDir(t, t.TempDir())

	if _, err := ensureServerBinary(context.Background()); err == nil {
		t.Fatal("expected checksum mismatch error")
	}

	cacheDir, err := userCacheDir()
	if err != nil {
		t.Fatalf("UserCacheDir: %v", err)
	}
	dest := filepath.Join(cacheDir, "eoka", "eoka-server-"+serverReleaseVersion+"-"+suffix)
	if _, err := os.Stat(dest); !os.IsNotExist(err) {
		t.Fatalf("expected no file left behind after checksum mismatch, stat err = %v", err)
	}
}

func TestResolveServerPathPrecedence(t *testing.T) {
	ctx := context.Background()

	t.Run("explicit wins", func(t *testing.T) {
		t.Setenv("EOKA_SERVER_BIN", "/from/env")
		got, err := resolveServerPath(ctx, "/from/explicit", true)
		if err != nil || got != "/from/explicit" {
			t.Fatalf("resolveServerPath: %q, %v", got, err)
		}
	})

	t.Run("env wins over PATH and download", func(t *testing.T) {
		t.Setenv("EOKA_SERVER_BIN", "/from/env")
		got, err := resolveServerPath(ctx, "", true)
		if err != nil || got != "/from/env" {
			t.Fatalf("resolveServerPath: %q, %v", got, err)
		}
	})

	t.Run("no download returns ErrServerBinaryNotFound", func(t *testing.T) {
		t.Setenv("EOKA_SERVER_BIN", "")
		t.Setenv("PATH", t.TempDir())
		_, err := resolveServerPath(ctx, "", false)
		if err != ErrServerBinaryNotFound {
			t.Fatalf("expected ErrServerBinaryNotFound, got %v", err)
		}
	})
}
