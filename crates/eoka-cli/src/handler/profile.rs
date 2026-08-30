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
        let mut candidates = vec![
            ".config/google-chrome",
            ".config/chromium",
            ".config/microsoft-edge",
        ];
        candidates.push(".var/app/com.google.Chrome/config/google-chrome");
        for candidate in candidates {
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
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&dst, fs::Permissions::from_mode(0o700));
    }
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

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::{Mutex, MutexGuard, OnceLock};

    fn home_lock() -> MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
    }

    fn temp_root(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("eoka-profile-test-{}-{}", std::process::id(), name));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn clone_is_owner_only() {
        let src = temp_root("src");
        let profile = src.join("google-chrome");
        fs::create_dir_all(profile.join("Default")).unwrap();
        fs::write(profile.join("Default").join("Cookies"), "cookie-bytes").unwrap();

        let dst = clone_profile_dir(&profile).unwrap();
        let mode = fs::metadata(&dst).unwrap().permissions().mode() & 0o777;
        assert_eq!(
            mode, 0o700,
            "clone dir holds the cookie store: must be 0700"
        );
        assert!(dst.join("Default").join("Cookies").exists());
        fs::remove_dir_all(&dst).unwrap();
        fs::remove_dir_all(&src).unwrap();
    }

    #[test]
    fn default_profile_dir_finds_flatpak_chrome() {
        let _guard = home_lock();
        let home = temp_root("home-flatpak");
        let flatpak = home.join(".var/app/com.google.Chrome/config/google-chrome");
        fs::create_dir_all(&flatpak).unwrap();
        unsafe {
            std::env::set_var("HOME", &home);
        }
        assert_eq!(default_profile_dir(), Some(flatpak));
        unsafe {
            std::env::remove_var("HOME");
        }
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn default_profile_dir_finds_plain_chrome_before_flatpak() {
        let _guard = home_lock();
        let home = temp_root("home-both");
        let plain = home.join(".config/google-chrome");
        let flatpak = home.join(".var/app/com.google.Chrome/config/google-chrome");
        fs::create_dir_all(&plain).unwrap();
        fs::create_dir_all(&flatpak).unwrap();
        unsafe {
            std::env::set_var("HOME", &home);
        }
        assert_eq!(default_profile_dir(), Some(plain));
        unsafe {
            std::env::remove_var("HOME");
        }
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn default_profile_dir_none_without_profiles() {
        let _guard = home_lock();
        let home = temp_root("home-empty");
        unsafe {
            std::env::set_var("HOME", &home);
        }
        assert_eq!(default_profile_dir(), None);
        unsafe {
            std::env::remove_var("HOME");
        }
        let _ = fs::remove_dir_all(&home);
    }
}
