use super::config::ResolvedConfig;
use std::path::{Path, PathBuf};

/// Generate bubblewrap (bwrap) command-line arguments from the sandbox config.
///
/// Strategy:
/// - Namespace isolation (PID, network, mount, UTS, IPC)
/// - System directories bind-mounted read-only
/// - Workspace writable (or specific allow_write paths)
/// - deny_read: resolve matching files, bind /dev/null over them
/// - deny_write: resolve matching files, bind read-only over them
/// - Network: --unshare-net to block all (proxy forwarded separately)
pub fn generate_args(config: &ResolvedConfig) -> Vec<String> {
    let mut args: Vec<String> = Vec::new();

    // ═══════════════════════════════════════════════════════
    // Namespace isolation
    // ═══════════════════════════════════════════════════════
    args.push("--unshare-pid".into());
    args.push("--unshare-uts".into());
    args.push("--unshare-ipc".into());

    // Network isolation:
    // - empty list: --unshare-net (kernel blocks everything)
    // - ["*"]: no isolation (unrestricted)
    // - specific domains: keep host network, proxy filters domains
    //   (full kernel isolation with proxy bridge planned for future)
    let network_unrestricted = config.network.iter().any(|d| d == "*");
    let network_has_domains = !config.network.is_empty() && !network_unrestricted;
    if config.network.is_empty() {
        // Empty = block all outbound at kernel level
        args.push("--unshare-net".into());
    } else if network_has_domains {
        // Specific domains: keep host network for proxy access
        // Proxy enforces domain filtering via env vars
    }
    // ["*"]: no --unshare-net, full access

    // Die with parent (cleanup on terminal close)
    args.push("--die-with-parent".into());

    // ═══════════════════════════════════════════════════════
    // System directories (read-only)
    // ═══════════════════════════════════════════════════════
    for dir in &["/usr", "/lib", "/lib64", "/lib32", "/bin", "/sbin", "/etc", "/opt"] {
        if Path::new(dir).exists() {
            args.push("--ro-bind".into());
            args.push(dir.to_string());
            args.push(dir.to_string());
        }
    }

    // /proc and /dev
    args.push("--proc".into());
    args.push("/proc".into());
    args.push("--dev".into());
    args.push("/dev".into());

    // ═══════════════════════════════════════════════════════
    // Writable paths (allow_write)
    // ═══════════════════════════════════════════════════════
    for path in &config.allow_write {
        if Path::new(path).exists() {
            args.push("--bind".into());
            args.push(path.clone());
            args.push(path.clone());
        }
    }

    // Ensure /tmp exists (many tools need it)
    if !config.allow_write.iter().any(|p| p == "/tmp") {
        args.push("--tmpfs".into());
        args.push("/tmp".into());
    }

    // System temp paths
    if let Ok(tmpdir) = std::env::var("TMPDIR") {
        if Path::new(&tmpdir).exists() && !config.allow_write.iter().any(|p| p == &tmpdir) {
            args.push("--bind".into());
            args.push(tmpdir.clone());
            args.push(tmpdir);
        }
    }

    // ═══════════════════════════════════════════════════════
    // Home directory — minimal shell config only
    // ═══════════════════════════════════════════════════════
    if let Some(home) = dirs::home_dir() {
        let home_str = home.to_string_lossy().to_string();

        // Create empty tmpfs over home (makes it invisible)
        args.push("--tmpfs".into());
        args.push(home_str.clone());

        // Bind shell configs (read-only)
        for dotfile in &[".bashrc", ".bash_profile", ".profile"] {
            let path = home.join(dotfile);
            if path.exists() {
                args.push("--ro-bind".into());
                args.push(path.to_string_lossy().to_string());
                args.push(format!("{}/{}", home_str, dotfile));
            }
        }
    }

    // ═══════════════════════════════════════════════════════
    // SSH agent socket (if enabled)
    // ═══════════════════════════════════════════════════════
    if config.options.ssh_agent {
        if let Ok(sock) = std::env::var("SSH_AUTH_SOCK") {
            if Path::new(&sock).exists() {
                args.push("--bind".into());
                args.push(sock.clone());
                args.push(sock);
            }
        }
    }

    // ═══════════════════════════════════════════════════════
    // deny_read: find matching files, bind /dev/null over them
    // This hides file contents (reads return empty/EOF)
    // ═══════════════════════════════════════════════════════
    if !config.deny_read.is_empty() {
        let matches = resolve_patterns(&config.workspace, &config.deny_read);
        for file_path in &matches {
            args.push("--ro-bind".into());
            args.push("/dev/null".into());
            args.push(file_path.clone());
        }
        if std::env::var("CATO_DEBUG").is_ok() && !matches.is_empty() {
            eprintln!("[cato] deny_read: hiding {} files via /dev/null bind", matches.len());
            for m in &matches {
                eprintln!("[cato]   {}", m);
            }
        }
    }

    // ═══════════════════════════════════════════════════════
    // deny_write: find matching files, re-bind as read-only
    // Existing files become read-only; new files with matching
    // names can still be created (bwrap limitation)
    // ═══════════════════════════════════════════════════════
    if !config.deny_write.is_empty() {
        let matches = resolve_patterns(&config.workspace, &config.deny_write);
        for file_path in &matches {
            args.push("--ro-bind".into());
            args.push(file_path.clone());
            args.push(file_path.clone());
        }
        if std::env::var("CATO_DEBUG").is_ok() {
            eprintln!("[cato] deny_write: {} patterns matched {} files", config.deny_write.len(), matches.len());
            for m in &matches {
                eprintln!("[cato]   ro-bind: {}", m);
            }
        }
    }

    // ═══════════════════════════════════════════════════════
    // Config file protection (read-only)
    // ═══════════════════════════════════════════════════════
    let config_path = format!("{}/.cato.toml", config.workspace);
    if Path::new(&config_path).exists() {
        args.push("--ro-bind".into());
        args.push(config_path.clone());
        args.push(config_path);
    }

    // ═══════════════════════════════════════════════════════
    // Working directory
    // ═══════════════════════════════════════════════════════
    args.push("--chdir".into());
    args.push(config.workspace.clone());

    // Separator
    args.push("--".into());

    args
}

/// Resolve glob patterns against the workspace directory.
/// Walks the workspace and returns absolute paths of files matching any pattern.
fn resolve_patterns(workspace: &str, patterns: &[String]) -> Vec<String> {
    let mut matches = Vec::new();
    let workspace_path = Path::new(workspace);

    if let Ok(entries) = walk_dir(workspace_path) {
        for entry in entries {
            let filename = entry.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let relative = entry.strip_prefix(workspace)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();

            for pattern in patterns {
                if matches_glob(&filename, &relative, pattern) {
                    matches.push(entry.to_string_lossy().to_string());
                    break;
                }
            }
        }
    }

    matches
}

/// Walk directory recursively, collecting file paths
fn walk_dir(path: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut results = Vec::new();
    if !path.is_dir() {
        return Ok(results);
    }
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            // Skip .git for performance
            if path.file_name().map(|n| n == ".git").unwrap_or(false) {
                continue;
            }
            if let Ok(mut sub) = walk_dir(&path) {
                results.append(&mut sub);
            }
        } else {
            results.push(path);
        }
    }
    Ok(results)
}

/// Check if a filename/path matches a deny pattern
fn matches_glob(filename: &str, relative_path: &str, pattern: &str) -> bool {
    if pattern.starts_with("**/") {
        // Recursive: **/.ssh/*
        let suffix = &pattern[3..];
        relative_path.contains(suffix) || filename == suffix
    } else if pattern.ends_with("/*") {
        // Directory glob: .github/*
        let dir = &pattern[..pattern.len() - 2];
        relative_path.starts_with(dir)
    } else if pattern.contains('*') {
        // General wildcard matching: *.env, *.env.*, *credentials*, *.key
        // Split on * and check all parts appear in order in the filename
        let parts: Vec<&str> = pattern.split('*').collect();
        let mut pos = 0;
        let mut first = true;
        for part in &parts {
            if part.is_empty() { first = false; continue; }
            if let Some(found) = filename[pos..].find(part) {
                // First non-empty part must be at start if pattern doesn't start with *
                if first && !pattern.starts_with('*') && found != 0 {
                    return false;
                }
                pos += found + part.len();
            } else {
                return false;
            }
            first = false;
        }
        // If pattern doesn't end with *, remaining filename must be consumed
        if !pattern.ends_with('*') && pos != filename.len() {
            return false;
        }
        true
    } else {
        // Exact filename: id_rsa, .git-credentials
        filename == pattern
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::config::{ResolvedConfig, SandboxOptions};

    #[test]
    fn test_basic_args() {
        let config = ResolvedConfig {
            workspace: "/home/user/project".into(),
            allow_write: vec!["/home/user/project".into(), "/tmp".into()],
            deny_write: vec![],
            deny_read: vec![],
            network: vec!["github.com".into()],
            tools: vec![],
            options: SandboxOptions { ssh_agent: false, allow_localhost: true },
        };

        let args = generate_args(&config);
        assert!(args.contains(&"--unshare-pid".to_string()));
        // Specific domains = keep host network for proxy, no --unshare-net
        assert!(!args.contains(&"--unshare-net".to_string()));
        assert!(args.contains(&"--die-with-parent".to_string()));

        let chdir_idx = args.iter().position(|a| a == "--chdir").unwrap();
        assert_eq!(args[chdir_idx + 1], "/home/user/project");
        assert_eq!(args.last().unwrap(), "--");
    }

    #[test]
    fn test_unrestricted_network() {
        let config = ResolvedConfig {
            workspace: "/home/user/project".into(),
            allow_write: vec!["/home/user/project".into()],
            deny_write: vec![],
            deny_read: vec![],
            network: vec!["*".into()],
            tools: vec![],
            options: SandboxOptions { ssh_agent: false, allow_localhost: true },
        };

        let args = generate_args(&config);
        assert!(!args.contains(&"--unshare-net".to_string()));
    }

    #[test]
    fn test_empty_network_blocked() {
        let config = ResolvedConfig {
            workspace: "/home/user/project".into(),
            allow_write: vec!["/home/user/project".into()],
            deny_write: vec![],
            deny_read: vec![],
            network: vec![],
            tools: vec![],
            options: SandboxOptions { ssh_agent: false, allow_localhost: true },
        };

        let args = generate_args(&config);
        // Empty network = --unshare-net (kernel blocks all)
        assert!(args.contains(&"--unshare-net".to_string()));
    }

    #[test]
    fn test_glob_matching() {
        assert!(matches_glob(".env", ".env", "*.env"));
        assert!(matches_glob("prod.env", "prod.env", "*.env"));
        assert!(matches_glob(".env.local", ".env.local", "*.env.*"));
        assert!(matches_glob("secret.key", "certs/secret.key", "*.key"));
        assert!(matches_glob("id_rsa", ".ssh/id_rsa", "id_rsa"));
        assert!(matches_glob("aws_credentials", "config/aws_credentials", "*credentials*"));
        assert!(matches_glob("deploy.yml", ".github/deploy.yml", ".github/*"));
        assert!(!matches_glob("main.rs", "src/main.rs", "*.env"));
        assert!(!matches_glob("README.md", "README.md", "*.key"));
    }
}
