package eoka

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"runtime"
	"strings"
)

const serverReleaseVersion = "0.1.2"

const defaultReleaseBaseURL = "https://github.com/shrimp-software/eoka-tools/releases/download"

var releaseBaseURL = defaultReleaseBaseURL
var userCacheDir = os.UserCacheDir

func assetSuffixFor(goos, goarch string) (string, error) {
	switch goos {
	case "linux":
		if goarch != "amd64" {
			return "", fmt.Errorf("eoka: no prebuilt eoka-server binary for linux/%s", goarch)
		}
		return "linux-amd64", nil
	case "darwin":
		if goarch != "amd64" && goarch != "arm64" {
			return "", fmt.Errorf("eoka: no prebuilt eoka-server binary for darwin/%s", goarch)
		}
		return "darwin-" + goarch, nil
	case "windows":
		if goarch != "amd64" {
			return "", fmt.Errorf("eoka: no prebuilt eoka-server binary for windows/%s", goarch)
		}
		return "windows-amd64.exe", nil
	default:
		return "", fmt.Errorf("eoka: no prebuilt eoka-server binary for %s/%s", goos, goarch)
	}
}

func releaseAssetURL(path string) string {
	return fmt.Sprintf("%s/eoka-server-v%s/%s", releaseBaseURL, serverReleaseVersion, path)
}

func fetchRelease(ctx context.Context, path string) ([]byte, error) {
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, releaseAssetURL(path), nil)
	if err != nil {
		return nil, err
	}
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		return nil, fmt.Errorf("eoka: downloading %s: %w", path, err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("eoka: downloading %s: unexpected status %s", path, resp.Status)
	}
	return io.ReadAll(resp.Body)
}

func checksumFor(checksums []byte, asset string) (string, error) {
	for _, line := range strings.Split(string(checksums), "\n") {
		fields := strings.Fields(line)
		if len(fields) == 2 && fields[1] == asset {
			return fields[0], nil
		}
	}
	return "", fmt.Errorf("eoka: no checksum entry for %s", asset)
}

func ensureServerBinary(ctx context.Context) (string, error) {
	suffix, err := assetSuffixFor(runtime.GOOS, runtime.GOARCH)
	if err != nil {
		return "", err
	}
	asset := "eoka-server-" + suffix

	cacheDir, err := userCacheDir()
	if err != nil {
		return "", fmt.Errorf("eoka: locating user cache dir: %w", err)
	}
	dest := filepath.Join(cacheDir, "eoka", "eoka-server-"+serverReleaseVersion+"-"+suffix)

	if info, err := os.Stat(dest); err == nil && info.Mode().IsRegular() {
		return dest, nil
	}

	checksums, err := fetchRelease(ctx, "checksums.txt")
	if err != nil {
		return "", err
	}
	wantSum, err := checksumFor(checksums, asset)
	if err != nil {
		return "", err
	}

	data, err := fetchRelease(ctx, asset)
	if err != nil {
		return "", err
	}

	sum := sha256.Sum256(data)
	gotSum := hex.EncodeToString(sum[:])
	if gotSum != wantSum {
		return "", fmt.Errorf("eoka: checksum mismatch for %s: got %s, want %s", asset, gotSum, wantSum)
	}

	if err := os.MkdirAll(filepath.Dir(dest), 0o755); err != nil {
		return "", fmt.Errorf("eoka: creating cache dir: %w", err)
	}
	tmp := dest + ".tmp"
	if err := os.WriteFile(tmp, data, 0o755); err != nil {
		return "", fmt.Errorf("eoka: writing downloaded binary: %w", err)
	}
	if err := os.Rename(tmp, dest); err != nil {
		os.Remove(tmp)
		return "", fmt.Errorf("eoka: installing downloaded binary: %w", err)
	}
	return dest, nil
}
