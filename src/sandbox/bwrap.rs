use super::config::ResolvedConfig;

/// Generate bubblewrap (bwrap) command-line arguments from the sandbox config.
///
/// Strategy:
/// - --unshare-all: isolate PID, network, mount, UTS namespaces
/// - --ro-bind: system directories (read-only)
/// - --bind: writable paths (allow_write)
/// - --unshare-net: block all network (proxy forwarded via unix socket)
/// - --proc/--dev: required for process operation
///
/// Landlock is applied separately inside the sandbox for deny_read/deny_write patterns.
pub fn generate_args(config: &ResolvedConfig) -> Vec<String> {
    let mut args: Vec<String> = Vec::new();

    // ═══════════════════════════════════════════════════════
    // Namespace isolation
    // ═══════════════════════════════════════════════════════
    args.push("--unshare-pid".into());
    args.push("--unshare-uts".into());
    args.push("--unshare-ipc".into());

    // Network: isolate unless unrestricted
    let network_unrestricted = config.network.iter().any(|d| d == "*");
    if !network_unrestricted {
        args.push("--unshare-net".into());
    }

    // Die with parent (cleanup on terminal close)
    args.push("--die-with-parent".into());

    // ═══════════════════════════════════════════════════════
    // System directories (read-only)
    // ═══════════════════════════════════════════════════════
    let ro_dirs = [
        "/usr", "/lib", "/lib64", "/bin", "/sbin",
        "/etc", "/opt",
    ];
    for dir in &ro_dirs {
        if std::path::Path::new(dir).exists() {
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
        if std::path::Path::new(path).exists() {
            args.push("--bind".into());
            args.push(path.clone());
            args.push(path.clone());
        }
    }

    // /tmp — always needed for build tools
    if !config.allow_write.iter().any(|p| p == "/tmp") {
        args.push("--tmpfs".into());
        args.push("/tmp".into());
    }

    // ═══════════════════════════════════════════════════════
    // Home directory — minimal shell config only
    // ═══════════════════════════════════════════════════════
    if let Some(home) = dirs::home_dir() {
        let home_str = home.to_string_lossy().to_string();

        // Create a minimal home inside sandbox
        args.push("--tmpfs".into());
        args.push(home_str.clone());

        // Bind shell configs (read-only)
        for dotfile in &[".bashrc", ".bash_profile", ".profile", ".zshrc", ".zprofile"] {
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
            let sock_path = std::path::Path::new(&sock);
            if sock_path.exists() {
                args.push("--bind".into());
                args.push(sock.clone());
                args.push(sock);
            }
        }
    }

    // ═══════════════════════════════════════════════════════
    // Network proxy socket (for domain filtering)
    // If network is restricted (not ["*"] and not empty),
    // the proxy socket is forwarded into the sandbox.
    // ═══════════════════════════════════════════════════════
    // Note: proxy socket binding is handled by the caller (run.rs)
    // which passes the socket path in env vars after generating args.

    // ═══════════════════════════════════════════════════════
    // Working directory
    // ═══════════════════════════════════════════════════════
    args.push("--chdir".into());
    args.push(config.workspace.clone());

    // ═══════════════════════════════════════════════════════
    // Config file protection (read-only)
    // ═══════════════════════════════════════════════════════
    let config_path = format!("{}/.cato.toml", config.workspace);
    if std::path::Path::new(&config_path).exists() {
        args.push("--ro-bind".into());
        args.push(config_path.clone());
        args.push(config_path);
    }

    // Separator between bwrap args and the command
    args.push("--".into());

    args
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

        // Should include namespace isolation
        assert!(args.contains(&"--unshare-pid".to_string()));
        assert!(args.contains(&"--unshare-net".to_string()));
        assert!(args.contains(&"--die-with-parent".to_string()));

        // Should include chdir to workspace
        let chdir_idx = args.iter().position(|a| a == "--chdir").unwrap();
        assert_eq!(args[chdir_idx + 1], "/home/user/project");

        // Should end with --
        assert_eq!(args.last().unwrap(), "--");
    }

    #[test]
    fn test_unrestricted_network_no_unshare() {
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
        // With ["*"], network should NOT be unshared
        assert!(!args.contains(&"--unshare-net".to_string()));
    }
}
