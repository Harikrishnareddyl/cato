use super::config::ResolvedConfig;

/// Generate a macOS Seatbelt (.sb) profile.
/// Deny-default: everything blocked, then explicitly allow what's needed.
/// System essentials borrowed from Anthropic's battle-tested sandbox-runtime.
pub fn generate(config: &ResolvedConfig, tool_paths: &[String]) -> String {
    let mut p: Vec<String> = Vec::new();

    p.push("(version 1)".into());
    p.push("(deny default)".into());
    p.push("".into());

    // ═══════════════════════════════════════════════════════
    // File metadata — needed for path traversal everywhere
    // stat/lstat only, no file contents exposed
    // ═══════════════════════════════════════════════════════
    p.push("; File metadata (path traversal)".into());
    p.push("(allow file-read-metadata)".into());
    // Root directory read — needed because /bin, /usr, /tmp etc are symlinks in root
    p.push("(allow file-read* (literal \"/\"))".into());
    p.push("".into());

    // ═══════════════════════════════════════════════════════
    // Process
    // ═══════════════════════════════════════════════════════
    p.push("; Process".into());
    p.push("(allow process-exec*)".into());
    p.push("(allow process-fork)".into());
    p.push("(allow process-info* (target same-sandbox))".into());
    p.push("(allow signal (target same-sandbox))".into());
    p.push("(allow mach-priv-task-port (target same-sandbox))".into());
    p.push("".into());

    // ═══════════════════════════════════════════════════════
    // Mach IPC — allow all lookups (services vary by macOS version)
    // ═══════════════════════════════════════════════════════
    p.push("; Mach IPC".into());
    p.push("(allow mach-lookup)".into());
    p.push("(allow user-preference-read)".into());
    p.push("".into());

    // ═══════════════════════════════════════════════════════
    // IPC
    // ═══════════════════════════════════════════════════════
    p.push("; IPC".into());
    p.push("(allow ipc-posix-shm)".into());
    p.push("(allow ipc-posix-sem)".into());
    p.push("(allow distributed-notification-post)".into());
    p.push("".into());

    // ═══════════════════════════════════════════════════════
    // sysctl — all reads (read-only system info, varies by macOS version)
    // ═══════════════════════════════════════════════════════
    p.push("; sysctl reads".into());
    p.push("(allow sysctl-read)".into());
    p.push("".into());

    // ═══════════════════════════════════════════════════════
    // Device files
    // ═══════════════════════════════════════════════════════
    p.push("; Device files".into());
    p.push("(allow file-ioctl (literal \"/dev/null\"))".into());
    p.push("(allow file-ioctl (literal \"/dev/zero\"))".into());
    p.push("(allow file-ioctl (literal \"/dev/random\"))".into());
    p.push("(allow file-ioctl (literal \"/dev/urandom\"))".into());
    p.push("(allow file-ioctl (literal \"/dev/tty\"))".into());
    p.push("(allow file-ioctl (literal \"/dev/dtracehelper\"))".into());
    p.push("(allow file-read-data file-write-data".into());
    p.push("  (require-all (literal \"/dev/null\") (vnode-type CHARACTER-DEVICE)))".into());
    p.push("".into());

    // ═══════════════════════════════════════════════════════
    // PTY (terminal)
    // ═══════════════════════════════════════════════════════
    p.push("; Pseudo-terminal support".into());
    p.push("(allow pseudo-tty)".into());
    p.push("(allow file-ioctl (literal \"/dev/ptmx\"))".into());
    p.push("(allow file-ioctl (regex #\"^/dev/ttys\"))".into());
    p.push("(allow file-read* file-write* (literal \"/dev/ptmx\"))".into());
    p.push("(allow file-read* file-write* (regex #\"^/dev/ttys\"))".into());
    p.push("".into());

    // ═══════════════════════════════════════════════════════
    // System reads (read-only) — minimum for shell + tools
    // ═══════════════════════════════════════════════════════
    p.push("; System binaries and libraries (read-only)".into());
    for path in &[
        "/usr", "/bin", "/sbin",
        "/Library",
        "/System",
        "/private/var/db",
        "/dev",
        "/etc",
        "/private/etc",
        "/opt/homebrew",
        "/usr/local",
    ] {
        p.push(format!("(allow file-read* (subpath \"{}\"))", path));
    }
    p.push("".into());

    // ═══════════════════════════════════════════════════════
    // Home directory — minimal shell config (read-only)
    // ═══════════════════════════════════════════════════════
    if let Some(home) = dirs::home_dir() {
        let h = home.to_string_lossy().to_string();
        p.push("; Home — shell config only (read-only)".into());
        // Allow reading the home dir itself (for ls, cd)
        p.push(format!("(allow file-read* (literal \"{}\"))", h));
        // Allow reading dotfiles in home root (shell configs like .zshrc, .zprofile)
        p.push(format!("(allow file-read* (regex #\"^{}/\\\\.[^/]*$\"))", regex_escape(&h)));
        // Shell history (needs write)
        p.push(format!("(allow file-read* file-write* (literal \"{}/.zsh_history\"))", h));
        p.push(format!("(allow file-read* file-write* (literal \"{}/.bash_history\"))", h));
        // Zsh cache/completions
        p.push(format!("(allow file-read* file-write* (subpath \"{}/.cache\"))", h));
        p.push(format!("(allow file-read* (subpath \"{}/.zsh\"))", h));
        p.push("".into());
    }

    // ═══════════════════════════════════════════════════════
    // Registered tool binaries (read-only, specific paths)
    // ═══════════════════════════════════════════════════════
    if !tool_paths.is_empty() {
        p.push("; Registered tool binaries".into());
        for path in tool_paths {
            p.push(format!("(allow file-read* (literal \"{}\"))", path));
        }
        p.push("".into());
    }

    // ═══════════════════════════════════════════════════════
    // Workspace — always readable
    // ═══════════════════════════════════════════════════════
    p.push("; Workspace (read access)".into());
    p.push(format!("(allow file-read* (subpath \"{}\"))", config.workspace));
    p.push("".into());

    // ═══════════════════════════════════════════════════════
    // allow_write — paths where writes are permitted
    // (deny-by-default for writes everywhere else)
    // ═══════════════════════════════════════════════════════
    p.push("; Writable paths (allow_write)".into());
    for path in &config.allow_write {
        p.push(format!("(allow file-write* (subpath \"{}\"))", path));
        // Also ensure read access for writable paths
        if *path != config.workspace {
            p.push(format!("(allow file-read* (subpath \"{}\"))", path));
        }
        if path == "/tmp" {
            p.push("(allow file-read* (subpath \"/private/tmp\"))".into());
            p.push("(allow file-write* (subpath \"/private/tmp\"))".into());
        }
    }
    // System temp paths (always needed for build tools)
    p.push("(allow file-read* file-write* (subpath \"/var/folders\"))".into());
    p.push("(allow file-read* file-write* (subpath \"/private/var/folders\"))".into());
    p.push("".into());

    // ═══════════════════════════════════════════════════════
    // deny_write — block writes to patterns within allow_write paths
    // Deny overrides allow (placed after allow rules)
    // ═══════════════════════════════════════════════════════
    if !config.deny_write.is_empty() {
        p.push("; Deny writes (overrides allow_write)".into());
        for pattern in &config.deny_write {
            if pattern.contains('*') || !pattern.contains('/') {
                let regex = glob_to_seatbelt_regex(pattern);
                p.push(format!(
                    "(deny file-write* (require-all (subpath \"{}\") (regex #\"{}\")))",
                    config.workspace, regex
                ));
            } else {
                p.push(format!("(deny file-write* (subpath \"{}\"))", pattern));
            }
        }
        p.push("".into());
    }

    // ═══════════════════════════════════════════════════════
    // Deny reading sensitive files within workspace/writable paths
    // Scoped to workspace + writable dirs so system files
    // (like /etc/ssl/cert.pem) aren't caught by *.pem patterns
    // ═══════════════════════════════════════════════════════
    if !config.deny_read.is_empty() {
        p.push("; Deny reading sensitive files (workspace-scoped)".into());
        for pattern in &config.deny_read {
            if pattern.contains('*') || !pattern.contains('/') {
                let regex = glob_to_seatbelt_regex(pattern);
                // Scope to workspace using require-all
                p.push(format!(
                    "(deny file-read-data (require-all (subpath \"{}\") (regex #\"{}\")))",
                    config.workspace, regex
                ));
            } else {
                p.push(format!("(deny file-read-data (subpath \"{}\"))", pattern));
            }
        }
        // Also deny writes to deny_read files (prevent overwriting host secrets)
        // and deny unlink (prevent deletion/rename bypass)
        for pattern in &config.deny_read {
            if pattern.contains('*') || !pattern.contains('/') {
                let regex = glob_to_seatbelt_regex(pattern);
                p.push(format!(
                    "(deny file-write-data (require-all (subpath \"{}\") (regex #\"{}\")))",
                    config.workspace, regex
                ));
                p.push(format!(
                    "(deny file-write-unlink (require-all (subpath \"{}\") (regex #\"{}\")))",
                    config.workspace, regex
                ));
            } else {
                p.push(format!("(deny file-write-data (subpath \"{}\"))", pattern));
                p.push(format!("(deny file-write-unlink (subpath \"{}\"))", pattern));
            }
        }
        p.push("".into());
    }

    // ═══════════════════════════════════════════════════════
    // Protect .cato.toml from writes
    // ═══════════════════════════════════════════════════════
    p.push("; Protect sandbox config".into());
    p.push(format!("(deny file-write* (literal \"{}/{}\"))", config.workspace, ".cato.toml"));
    p.push("".into());

    // ═══════════════════════════════════════════════════════
    // SSH agent socket
    // ═══════════════════════════════════════════════════════
    if config.options.ssh_agent {
        if let Ok(sock) = std::env::var("SSH_AUTH_SOCK") {
            p.push("; SSH agent".into());
            p.push("(allow system-socket (socket-domain AF_UNIX))".into());
            if let Some(parent) = std::path::Path::new(&sock).parent() {
                p.push(format!("(allow file-read* file-write* (subpath \"{}\"))", parent.display()));
            }
            let sock_parent = std::path::Path::new(&sock).parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();
            p.push(format!("(allow network-outbound (remote unix-socket (subpath \"{}\")))", sock_parent));
            p.push("".into());
        }
    }

    // ═══════════════════════════════════════════════════════
    // Network
    // Strategy: Seatbelt can only allow/deny ALL outbound (host must be * or localhost).
    // For domain filtering: block all outbound at kernel level, allow only localhost.
    // A local proxy on localhost handles domain allow/deny.
    // ═══════════════════════════════════════════════════════
    p.push("; Network".into());

    // Always allow localhost (proxy runs here, dev servers, etc.)
    p.push("(allow network-bind (local ip \"*:*\"))".into());
    p.push("(allow network-inbound (local ip \"*:*\"))".into());
    p.push("(allow network-outbound (remote ip \"localhost:*\"))".into());

    let network_unrestricted = config.network.iter().any(|d| d == "*");

    if network_unrestricted {
        // Explicit ["*"] = unrestricted network
        p.push("(allow network*)".into());
    } else {
        // Deny by default (empty list = no outbound, non-empty = proxy filters)
        // The proxy (running on localhost) enforces domain filtering
        // DNS also blocked — proxy handles resolution
        p.push("; All outbound blocked except localhost (proxy enforces domain list)".into());
    }
    // System socket needed for any network operations
    p.push("(allow system-socket)".into());
    p.push("".into());

    p.push("; End of cato sandbox profile".into());

    p.join("\n")
}

fn regex_escape(s: &str) -> String {
    s.replace('/', "\\/")
}

/// Convert glob to Seatbelt regex
fn glob_to_seatbelt_regex(pattern: &str) -> String {
    let mut regex = String::new();

    if !pattern.starts_with('/') {
        regex.push_str(".*/");
    }

    let chars: Vec<char> = pattern.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '*' if i + 1 < chars.len() && chars[i + 1] == '*' => {
                regex.push_str(".*");
                i += 2;
                if i < chars.len() && chars[i] == '/' { i += 1; }
                continue;
            }
            '*' => regex.push_str("[^/]*"),
            '.' => regex.push_str("\\."),
            '?' => regex.push_str("[^/]"),
            c @ ('[' | ']' | '(' | ')' | '{' | '}' | '+' | '^' | '$' | '|') => {
                regex.push('\\');
                regex.push(c);
            }
            c => regex.push(c),
        }
        i += 1;
    }

    regex
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glob_env() {
        let r = glob_to_seatbelt_regex("*.env");
        assert!(r.contains("[^/]*"));
        assert!(r.contains("\\.env"));
        assert!(r.starts_with(".*/"));
    }

    #[test]
    fn test_glob_double_star() {
        let r = glob_to_seatbelt_regex("**/.ssh/*");
        assert!(r.contains(".*"));
    }

    #[test]
    fn test_glob_literal() {
        let r = glob_to_seatbelt_regex("id_rsa");
        assert!(r.contains("id_rsa"));
        assert!(r.starts_with(".*/"));
    }
}
