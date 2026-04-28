/// Landlock LSM support for fine-grained file access control on Linux.
///
/// Landlock (Linux 5.13+) allows unprivileged processes to restrict their own
/// filesystem access. We use it inside the bwrap sandbox to enforce deny_read
/// and deny_write patterns — giving us glob-pattern granularity similar to
/// macOS Seatbelt within mounted directories.
///
/// Strategy:
/// 1. Create a Landlock ruleset allowing full access to workspace
/// 2. For each deny_read pattern: resolve matching files, remove read access
/// 3. For each deny_write pattern: resolve matching files, remove write access
/// 4. Restrict self (all future operations bound by these rules)
///
/// If Landlock is not available (kernel < 5.13), we fall back to bwrap-only
/// isolation (no glob patterns, whole-directory control only).

use super::config::ResolvedConfig;

/// Check if Landlock is available on this kernel
pub fn is_available() -> bool {
    #[cfg(target_os = "linux")]
    {
        // Try to create a landlock ruleset — if it fails, not available
        // Landlock ABI version 1+ needed
        unsafe {
            let attr = libc::landlock_ruleset_attr {
                handled_access_fs: LANDLOCK_ACCESS_FS_ALL,
            };
            let fd = libc::syscall(
                libc::SYS_landlock_create_ruleset,
                &attr as *const _,
                std::mem::size_of::<libc::landlock_ruleset_attr>(),
                0u32,
            );
            if fd >= 0 {
                libc::close(fd as i32);
                return true;
            }
        }
        false
    }
    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}

/// Apply Landlock rules based on the config.
/// Must be called INSIDE the sandbox (after fork, before exec or in the wrapper).
///
/// Returns Ok(()) on success, Err with message on failure.
/// If Landlock is not available, returns Ok(()) silently (graceful degradation).
pub fn apply_rules(config: &ResolvedConfig) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        if !is_available() {
            // Landlock not supported — skip silently
            // bwrap still provides directory-level isolation
            return Ok(());
        }

        // TODO: Implement Landlock ruleset
        // 1. Create ruleset with LANDLOCK_ACCESS_FS_* flags
        // 2. Add rules for workspace (allow read+write)
        // 3. For deny_read patterns: resolve glob, add rules removing read
        // 4. For deny_write patterns: resolve glob, add rules removing write
        // 5. landlock_restrict_self()

        let _ = config; // suppress unused warning until implemented
        Ok(())
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = config;
        Ok(())
    }
}

// Landlock constants (not yet in libc crate stable)
#[cfg(target_os = "linux")]
const LANDLOCK_ACCESS_FS_ALL: u64 = 0x1FFF; // All filesystem access flags combined

// Placeholder for libc landlock types (will use landlock crate or raw syscalls)
#[cfg(target_os = "linux")]
mod libc {
    #[repr(C)]
    pub struct landlock_ruleset_attr {
        pub handled_access_fs: u64,
    }

    pub const SYS_landlock_create_ruleset: i64 = 444; // x86_64

    extern "C" {
        pub fn syscall(num: i64, ...) -> i64;
        pub fn close(fd: i32) -> i32;
    }
}

/// Resolve glob patterns to actual file paths within the workspace.
/// Used to find files matching deny_read/deny_write patterns.
#[cfg(target_os = "linux")]
fn resolve_glob_pattern(workspace: &str, pattern: &str) -> Vec<String> {
    use std::path::Path;

    let mut matches = Vec::new();

    // Simple pattern resolution — walk workspace and match
    // For now, handle common patterns:
    // "*.env" — match files ending with .env in any directory
    // "*.key" — match files ending with .key
    // "id_rsa" — exact filename match anywhere

    if let Ok(entries) = walkdir(Path::new(workspace)) {
        for entry in entries {
            let filename = entry.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let relative = entry.strip_prefix(workspace)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();

            if matches_pattern(&filename, &relative, pattern) {
                matches.push(entry.to_string_lossy().to_string());
            }
        }
    }

    matches
}

/// Walk directory recursively
#[cfg(target_os = "linux")]
fn walkdir(path: &std::path::Path) -> std::io::Result<Vec<std::path::PathBuf>> {
    let mut results = Vec::new();
    if path.is_dir() {
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                if let Ok(mut sub) = walkdir(&path) {
                    results.append(&mut sub);
                }
            } else {
                results.push(path);
            }
        }
    }
    Ok(results)
}

/// Check if a filename/path matches a deny pattern
#[cfg(target_os = "linux")]
fn matches_pattern(filename: &str, relative_path: &str, pattern: &str) -> bool {
    if pattern.starts_with("**/") {
        // Recursive match
        let suffix = &pattern[3..];
        relative_path.contains(suffix) || filename == suffix
    } else if pattern.starts_with("*.") {
        // Extension match: *.env, *.key, etc.
        let ext = &pattern[1..]; // .env, .key
        filename.ends_with(ext)
    } else if pattern.contains('*') {
        // General glob — *credentials*, etc.
        let parts: Vec<&str> = pattern.split('*').collect();
        if parts.len() == 2 {
            filename.contains(parts[0]) && filename.contains(parts[1])
        } else if parts.len() == 3 && parts[0].is_empty() && parts[2].is_empty() {
            // *word* pattern
            filename.contains(parts[1])
        } else {
            false
        }
    } else {
        // Exact filename match
        filename == pattern
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_landlock_availability_check() {
        // On macOS this should return false
        #[cfg(target_os = "macos")]
        assert_eq!(super::is_available(), false);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_pattern_matching() {
        assert!(super::matches_pattern(".env", ".env", "*.env"));
        assert!(super::matches_pattern("prod.env", "prod.env", "*.env"));
        assert!(super::matches_pattern("secret.key", "certs/secret.key", "*.key"));
        assert!(super::matches_pattern("id_rsa", ".ssh/id_rsa", "id_rsa"));
        assert!(super::matches_pattern("aws_credentials", "config/aws_credentials", "*credentials*"));
        assert!(!super::matches_pattern("main.rs", "src/main.rs", "*.env"));
    }
}
