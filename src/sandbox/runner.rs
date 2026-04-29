use super::config::ResolvedConfig;
use std::path::Path;

/// Run the sandbox — dispatches to platform-specific implementation
#[cfg(target_os = "macos")]
pub fn run(
    resolved: &ResolvedConfig,
    profile_path: &Path,
    command: Option<Vec<String>>,
    env_vars: Vec<(String, String)>,
) -> i32 {
    run_macos(resolved, profile_path, command, env_vars)
}

#[cfg(target_os = "linux")]
pub fn run(
    resolved: &ResolvedConfig,
    profile_path: &Path,
    command: Option<Vec<String>>,
    env_vars: Vec<(String, String)>,
) -> i32 {
    let _ = profile_path; // not used on Linux
    run_linux(resolved, command, env_vars)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn run(
    _resolved: &ResolvedConfig,
    _profile_path: &Path,
    _command: Option<Vec<String>>,
    _env_vars: Vec<(String, String)>,
) -> i32 {
    eprintln!("[cato] Unsupported platform. Cato requires macOS or Linux.");
    1
}

/// macOS: sandbox-exec with Seatbelt profile
#[cfg(target_os = "macos")]
fn run_macos(
    resolved: &ResolvedConfig,
    profile_path: &Path,
    command: Option<Vec<String>>,
    env_vars: Vec<(String, String)>,
) -> i32 {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());

    let mut cmd = std::process::Command::new("/usr/bin/sandbox-exec");
    cmd.arg("-f").arg(profile_path);

    if let Some(ref args) = command {
        if let Some(first) = args.first() {
            cmd.arg(first);
            for arg in &args[1..] {
                cmd.arg(arg);
            }
        }
    } else {
        cmd.arg(&shell);
    }

    // Clear environment and set only what we want
    cmd.env_clear();
    set_common_env(&mut cmd, resolved, &env_vars);

    // macOS-specific: custom prompt via ZDOTDIR
    let workspace_name = workspace_short_name(&resolved.workspace);
    let zdotdir = std::env::temp_dir().join(format!("cato-zsh-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&zdotdir);
    let zshrc_content = format!(
        "# Cato sandbox shell\n\
         [[ -f ~/.zshrc ]] && source ~/.zshrc 2>/dev/null\n\
         PROMPT='%F{{yellow}}🔒 {}%f %~ $ '\n",
        workspace_name
    );
    let _ = std::fs::write(zdotdir.join(".zshrc"), &zshrc_content);
    cmd.env("ZDOTDIR", &zdotdir);

    cmd.current_dir(&resolved.workspace);

    let exit_code = execute_cmd(&mut cmd, "sandbox-exec", profile_path, &command, &shell);

    let _ = std::fs::remove_dir_all(&zdotdir);
    exit_code
}

/// Linux: bubblewrap + Landlock
#[cfg(target_os = "linux")]
fn run_linux(
    resolved: &ResolvedConfig,
    command: Option<Vec<String>>,
    env_vars: Vec<(String, String)>,
) -> i32 {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());

    // Check bubblewrap is available
    if !Path::new("/usr/bin/bwrap").exists() && which("bwrap").is_none() {
        eprintln!("[cato] bubblewrap (bwrap) not found.");
        eprintln!("  Install: sudo apt install bubblewrap  (Debian/Ubuntu)");
        eprintln!("           sudo dnf install bubblewrap  (Fedora)");
        return 1;
    }

    // Set up socat bridge if network domains configured
    let network_needs_proxy = !resolved.network.is_empty()
        && !resolved.network.iter().any(|d| d == "*");
    let has_socat = which("socat").is_some();

    let proxy_bridge = if network_needs_proxy && has_socat {
        // Find proxy port from injected env vars
        let proxy_port = env_vars.iter()
            .find(|(k, _)| k == "http_proxy")
            .and_then(|(_, v)| v.rsplit(':').next())
            .and_then(|p| p.parse::<u16>().ok());

        if let Some(port) = proxy_port {
            let socket_id = std::process::id();
            let socket_path = format!("/tmp/cato-http-{}.sock", socket_id);
            let inner_port = 3128u16;

            // Host-side socat: Unix socket → TCP proxy
            // (matches Anthropic: UNIX-LISTEN + TCP with keepalive)
            let child = std::process::Command::new("socat")
                .arg(format!("UNIX-LISTEN:{},fork,reuseaddr", socket_path))
                .arg(format!("TCP:localhost:{},keepalive,keepidle=10,keepintvl=5,keepcnt=3", port))
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn();

            match child {
                Ok(_) => {
                    // Poll for socket existence (matches Anthropic: 5 attempts, backoff)
                    let mut ready = false;
                    for i in 0..10 {
                        if Path::new(&socket_path).exists() {
                            ready = true;
                            if std::env::var("CATO_DEBUG").is_ok() {
                                eprintln!("[cato] socat bridge ready after {} attempts", i + 1);
                            }
                            break;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(50 * (i as u64 + 1)));
                    }
                    if !ready {
                        eprintln!("[cato] warning: socat bridge socket not created");
                        None
                    } else {
                        Some(super::bwrap::ProxyBridge { socket_path, inner_port })
                    }
                }
                Err(e) => {
                    eprintln!("[cato] warning: failed to start socat: {}", e);
                    None
                }
            }
        } else {
            None
        }
    } else if network_needs_proxy && !has_socat {
        eprintln!("[cato] warning: socat not found — network domain filtering disabled");
        eprintln!("  Install: sudo apt install socat");
        None
    } else {
        None
    };

    // Generate bwrap arguments
    let bwrap_args = super::bwrap::generate_args(resolved, proxy_bridge.as_ref());

    let mut cmd = std::process::Command::new("bwrap");
    for arg in &bwrap_args {
        cmd.arg(arg);
    }

    // If proxy bridge active, wrap command with inner socat
    // (matches Anthropic: socat TCP-LISTEN → UNIX-CONNECT, trap, eval)
    if let Some(ref bridge) = proxy_bridge {
        let user_cmd = if let Some(ref args) = command {
            args.join(" ")
        } else {
            shell.clone()
        };

        cmd.arg("/bin/sh");
        cmd.arg("-c");
        cmd.arg(format!(
            "socat TCP-LISTEN:{port},fork,reuseaddr UNIX-CONNECT:{sock} >/dev/null 2>&1 &\n\
             trap 'kill %1 2>/dev/null; exit' EXIT\n\
             sleep 0.2\n\
             eval {cmd}",
            port = bridge.inner_port,
            sock = bridge.socket_path,
            cmd = shell_quote(&user_cmd),
        ));
    } else {
        if let Some(ref args) = command {
            for arg in args {
                cmd.arg(arg);
            }
        } else {
            cmd.arg(&shell);
        }
    }

    // Clear environment and set only what we want
    cmd.env_clear();

    if proxy_bridge.is_some() {
        // Filter out host proxy vars, set inner proxy vars instead
        let filtered: Vec<(String, String)> = env_vars.into_iter()
            .filter(|(k, _)| {
                !k.eq_ignore_ascii_case("http_proxy")
                    && !k.eq_ignore_ascii_case("https_proxy")
                    && !k.eq_ignore_ascii_case("no_proxy")
            })
            .collect();
        set_common_env(&mut cmd, resolved, &filtered);
        // Inner proxy vars point to the socat listener inside sandbox
        cmd.env("http_proxy", "http://127.0.0.1:3128");
        cmd.env("https_proxy", "http://127.0.0.1:3128");
        cmd.env("HTTP_PROXY", "http://127.0.0.1:3128");
        cmd.env("HTTPS_PROXY", "http://127.0.0.1:3128");
        cmd.env("no_proxy", "localhost,127.0.0.1,::1");
        cmd.env("NO_PROXY", "localhost,127.0.0.1,::1");
    } else {
        set_common_env(&mut cmd, resolved, &env_vars);
    }

    // Linux-specific: PS1 prompt
    let workspace_name = workspace_short_name(&resolved.workspace);
    cmd.env("PS1", format!("🔒 {} \\w $ ", workspace_name));

    // LD_PRELOAD deny library — catches new files matching deny patterns
    // Look for libcato_deny.so next to the cato binary or in known paths
    let preload_lib = find_preload_lib();
    if let Some(ref lib_path) = preload_lib {
        cmd.env("LD_PRELOAD", lib_path);
        // Pass deny patterns as comma-separated env vars
        if !resolved.deny_read.is_empty() {
            cmd.env("CATO_DENY_READ", resolved.deny_read.join(","));
        }
        if !resolved.deny_write.is_empty() {
            cmd.env("CATO_DENY_WRITE", resolved.deny_write.join(","));
        }
        if std::env::var("CATO_DEBUG").is_ok() {
            eprintln!("[cato] LD_PRELOAD: {}", lib_path);
        }
    }

    cmd.current_dir(&resolved.workspace);

    let exit_code = execute_cmd(&mut cmd, "bwrap", Path::new(""), &command, &shell);

    // Cleanup bridge socket
    if let Some(ref bridge) = proxy_bridge {
        let _ = std::fs::remove_file(&bridge.socket_path);
    }

    exit_code
}

/// Set environment variables common to all platforms
fn set_common_env(
    cmd: &mut std::process::Command,
    resolved: &ResolvedConfig,
    env_vars: &[(String, String)],
) {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());

    cmd.env("HOME", std::env::var("HOME").unwrap_or_default());
    cmd.env("USER", std::env::var("USER").unwrap_or_default());
    cmd.env("SHELL", &shell);
    cmd.env("TERM", std::env::var("TERM").unwrap_or_else(|_| "xterm-256color".to_string()));
    cmd.env("LANG", std::env::var("LANG").unwrap_or_else(|_| "en_US.UTF-8".to_string()));
    cmd.env("PATH", std::env::var("PATH").unwrap_or_default());
    cmd.env("CATO_SANDBOX", "1");
    cmd.env("CATO_WORKSPACE", workspace_short_name(&resolved.workspace));

    // Injected secrets
    for (key, value) in env_vars {
        cmd.env(key, value);
    }

    // SSH agent forwarding
    if resolved.options.ssh_agent {
        if let Ok(sock) = std::env::var("SSH_AUTH_SOCK") {
            cmd.env("SSH_AUTH_SOCK", sock);
        }
    }
}

/// Execute the sandbox command and return exit code
fn execute_cmd(
    cmd: &mut std::process::Command,
    tool_name: &str,
    profile_path: &Path,
    command: &Option<Vec<String>>,
    shell: &str,
) -> i32 {
    let debug = std::env::var("CATO_DEBUG").is_ok();
    if debug {
        eprintln!("[cato] Running: {} -f {} {:?}", tool_name, profile_path.display(),
            command.as_ref().map(|c| c.join(" ")).unwrap_or_else(|| shell.to_string()));
    }

    if debug {
        let output = cmd.output();
        match output {
            Ok(output) => {
                use std::io::Write;
                let _ = std::io::stdout().write_all(&output.stdout);
                let _ = std::io::stderr().write_all(&output.stderr);
                let code = output.status.code().unwrap_or(1);
                eprintln!("[cato] {} exited with code: {}", tool_name, code);
                code
            }
            Err(e) => {
                eprintln!("[cato] Failed to start {}: {}", tool_name, e);
                1
            }
        }
    } else {
        match cmd.status() {
            Ok(status) => status.code().unwrap_or(1),
            Err(e) => {
                eprintln!("[cato] Failed to start {}: {}", tool_name, e);
                if e.kind() == std::io::ErrorKind::NotFound {
                    eprintln!("[cato] {} not found.", tool_name);
                }
                1
            }
        }
    }
}

fn workspace_short_name(workspace: &str) -> String {
    Path::new(workspace)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "sandbox".to_string())
}

/// Simple shell quoting — wraps in single quotes, escaping inner single quotes
#[cfg(target_os = "linux")]
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Find the LD_PRELOAD deny library (libcato_deny.so)
/// Searches next to the cato binary, then common install paths
#[cfg(target_os = "linux")]
fn find_preload_lib() -> Option<String> {
    // Next to the cato binary
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let lib = dir.join("libcato_deny.so");
            if lib.exists() {
                return Some(lib.to_string_lossy().to_string());
            }
        }
    }
    // Common install locations
    for path in &[
        "/usr/lib/cato/libcato_deny.so",
        "/usr/local/lib/cato/libcato_deny.so",
    ] {
        if Path::new(path).exists() {
            return Some(path.to_string());
        }
    }
    None
}

fn which(name: &str) -> Option<String> {
    std::process::Command::new("which")
        .arg(name)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
}
