//! Cold profile clone.
//!
//! Copies a Chrome user-data-dir to a tempdir so eoka can launch headless
//! against it without conflicting with a running Chrome (which holds a
//! SingletonSocket lock on its profile).
//!
//! Caveats:
//! - Cookie decryption uses the OS keyring (libsecret/keyring on Linux,
//!   Keychain on macOS, DPAPI/App-Bound on Windows). Running as the same
//!   user typically Just Works.
//! - Chrome 127+ on Windows binds cookies to the Chrome binary path via
//!   App-Bound Encryption. Launching a different binary against the
//!   cloned profile may silently lose those cookies.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Find the platform-default Chrome profile directory.
pub fn default_profile_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    if cfg!(target_os = "linux") {
        let h = home?;
        // Try Chrome, Chromium, then Edge in that order.
        for candidate in [
            ".config/google-chrome",
            ".config/chromium",
            ".config/microsoft-edge",
        ] {
            let p = h.join(candidate);
            if p.exists() {
                return Some(p);
            }
        }
        None
    } else if cfg!(target_os = "macos") {
        let h = home?;
        for candidate in [
            "Library/Application Support/Google/Chrome",
            "Library/Application Support/Chromium",
            "Library/Application Support/Microsoft Edge",
        ] {
            let p = h.join(candidate);
            if p.exists() {
                return Some(p);
            }
        }
        None
    } else if cfg!(target_os = "windows") {
        std::env::var_os("LOCALAPPDATA").map(|d| {
            PathBuf::from(d)
                .join("Google")
                .join("Chrome")
                .join("User Data")
        })
    } else {
        None
    }
}

/// Copy a Chrome user-data-dir to `<tmp>/eoka-profile-clone-<pid>-<n>`.
/// Returns the destination path. Skips known cache/lock paths to keep the
/// copy small and avoid stomping on a live Chrome's locks.
pub fn clone_profile_dir(src: &Path) -> io::Result<PathBuf> {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dst = std::env::temp_dir().join(format!("eoka-profile-clone-{}-{}", std::process::id(), n));
    fs::create_dir_all(&dst)?;
    copy_recursive(src, &dst)?;
    Ok(dst)
}

fn should_skip(name: &str) -> bool {
    matches!(
        name,
        // Live-Chrome locks; copying these confuses the launched instance.
        "SingletonSocket" | "SingletonCookie" | "SingletonLock"
        // Crash dumps and large caches we don't need for cookies/storage.
        | "Crashpad" | "Crash Reports" | "GrShaderCache" | "ShaderCache"
        | "GraphiteDawnCache" | "component_crx_cache"
        | "Service Worker" | "Code Cache" | "CacheStorage" | "Cache"
        // Per-extension state we don't need for auth.
        | "Extension Rules" | "Extension State"
    )
}

fn copy_recursive(src: &Path, dst: &Path) -> io::Result<()> {
    if !src.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if should_skip(&name_str) {
            continue;
        }
        let from = entry.path();
        let to = dst.join(&name);
        let ft = entry.file_type()?;
        if ft.is_dir() {
            fs::create_dir_all(&to)?;
            copy_recursive(&from, &to)?;
        } else if ft.is_file() {
            // Best-effort copy; skip unreadable files (e.g. a still-locked DB)
            // and let Chrome regenerate them.
            if let Err(e) = fs::copy(&from, &to) {
                eprintln!("[eoka] skipping {}: {}", from.display(), e);
            }
        }
        // Symlinks: skip — too easy to escape the tempdir.
    }
    Ok(())
}
