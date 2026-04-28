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

    // Set up proxy bridge if network domains are configured
    let network_needs_proxy = !resolved.network.is_empty()
        && !resolved.network.iter().any(|d| d == "*");
    let has_socat = which("socat").is_some();

    let proxy_bridge = if network_needs_proxy && has_socat {
        let socket_path = format!("/tmp/cato-proxy-{}.sock", std::process::id());
        let inner_port = 3128u16;

        // Find the proxy port from env vars
        let proxy_port = env_vars.iter()
            .find(|(k, _)| k == "http_proxy" || k == "HTTP_PROXY")
            .and_then(|(_, v)| v.rsplit(':').next())
            .and_then(|p| p.parse::<u16>().ok());

        if let Some(port) = proxy_port {
            // Start host-side socat: Unix socket → TCP proxy
            let _ = std::process::Command::new("socat")
                .arg(format!("UNIX-LISTEN:{},fork,reuseaddr,mode=777", socket_path))
                .arg(format!("TCP:127.0.0.1:{}", port))
                .spawn();

            // Give socat a moment to create the socket
            std::thread::sleep(std::time::Duration::from_millis(100));

            Some(super::bwrap::ProxyBridge {
                socket_path,
                inner_port,
            })
        } else {
            None
        }
    } else if network_needs_proxy && !has_socat {
        eprintln!("[cato] warning: socat not found — network domain filtering reduced");
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
    if let Some(ref bridge) = proxy_bridge {
        let user_cmd = if let Some(ref args) = command {
            args.join(" ")
        } else {
            shell.clone()
        };

        // Inner socat: listen on TCP port, forward to Unix socket
        // Then run the user's command
        cmd.arg("/bin/sh");
        cmd.arg("-c");
        cmd.arg(format!(
            "socat TCP-LISTEN:{port},fork,reuseaddr,bind=127.0.0.1 UNIX-CONNECT:{sock} &\n\
             SOCAT_PID=$!\n\
             sleep 0.1\n\
             export http_proxy=http://127.0.0.1:{port}\n\
             export https_proxy=http://127.0.0.1:{port}\n\
             export HTTP_PROXY=http://127.0.0.1:{port}\n\
             export HTTPS_PROXY=http://127.0.0.1:{port}\n\
             export no_proxy=localhost,127.0.0.1,::1\n\
             export NO_PROXY=localhost,127.0.0.1,::1\n\
             {cmd}\n\
             EXIT=$?\n\
             kill $SOCAT_PID 2>/dev/null\n\
             exit $EXIT",
            port = bridge.inner_port,
            sock = bridge.socket_path,
            cmd = user_cmd,
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

    // When proxy bridge is active, DON'T set host proxy vars
    // (inner socat sets them correctly)
    if proxy_bridge.is_some() {
        let filtered_vars: Vec<(String, String)> = env_vars.into_iter()
            .filter(|(k, _)| {
                !k.eq_ignore_ascii_case("http_proxy")
                    && !k.eq_ignore_ascii_case("https_proxy")
                    && !k.eq_ignore_ascii_case("no_proxy")
            })
            .collect();
        set_common_env(&mut cmd, resolved, &filtered_vars);
    } else {
        set_common_env(&mut cmd, resolved, &env_vars);
    }

    // Linux-specific: PS1 prompt
    let workspace_name = workspace_short_name(&resolved.workspace);
    cmd.env("PS1", format!("🔒 {} \\w $ ", workspace_name));

    cmd.current_dir(&resolved.workspace);

    let exit_code = execute_cmd(&mut cmd, "bwrap", Path::new(""), &command, &shell);

    // Cleanup proxy bridge socket
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

fn which(name: &str) -> Option<String> {
    std::process::Command::new("which")
        .arg(name)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
}
