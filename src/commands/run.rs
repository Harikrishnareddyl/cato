use crate::sandbox;

pub fn run(command: Option<Vec<String>>, ephemeral: bool) {
    // Check if already in a sandbox
    if std::env::var("CATO_SANDBOX").is_ok() {
        eprintln!("[cato] Already inside a sandbox. Exit first.");
        std::process::exit(1);
    }

    let cwd = std::env::current_dir().unwrap_or_else(|_| {
        eprintln!("[cato] Cannot determine current directory");
        std::process::exit(1);
    });

    // Find .cato.toml
    let config_path = cwd.join(".cato.toml");
    if !config_path.exists() {
        eprintln!("[cato] No .cato.toml found in current directory.");
        eprintln!("  Run `cato init` to create one.");
        std::process::exit(1);
    }

    // Parse sandbox config
    let sandbox_config = match sandbox::config::load(&config_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[cato] {}", e);
            std::process::exit(1);
        }
    };

    // Determine workspace (use ephemeral copy if requested)
    let workspace = if ephemeral {
        let tmp = std::env::temp_dir().join(format!("cato-eph-{}", std::process::id()));
        eprintln!("[cato] Ephemeral mode: copying workspace to {}", tmp.display());
        copy_dir_recursive(&cwd, &tmp).unwrap_or_else(|e| {
            eprintln!("[cato] Failed to copy workspace: {}", e);
            std::process::exit(1);
        });
        tmp
    } else {
        cwd.clone()
    };

    // Resolve config (expand tokens, resolve paths)
    let resolved = sandbox::config::resolve(&sandbox_config, &workspace);

    // Check tool requirements
    let missing_tools = check_tools(&sandbox_config.tools);
    if !missing_tools.is_empty() {
        eprintln!("[cato] Missing tools:");
        for tool in &missing_tools {
            eprintln!("  \x1b[31m✗\x1b[0m {} — run: cato tool add {}", tool, tool);
        }
        std::process::exit(1);
    }

    // Resolve secrets
    let mut env_vars = resolve_secrets(&sandbox_config);

    // Resolve tool paths for sandbox profile
    let store_tools = crate::commands::tool::load_store_tools();
    let tool_paths: Vec<String> = sandbox_config.tools.iter()
        .filter_map(|t| {
            store_tools.get(t).cloned().or_else(|| {
                std::process::Command::new("which").arg(t).output().ok()
                    .filter(|o| o.status.success())
                    .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            })
        })
        .collect();

    // Start network proxy if domain filtering is configured
    // Empty network = blocked (no proxy needed, kernel blocks all)
    // ["*"] = unrestricted (no proxy needed)
    // ["github.com", ...] = proxy filters by domain
    let audit_path = crate::audit::log_path();
    let network_needs_proxy = !resolved.network.is_empty()
        && !resolved.network.iter().any(|d| d == "*");
    let _proxy = if network_needs_proxy {
        match sandbox::proxy::NetworkProxy::start(
            resolved.network.clone(),
            resolved.workspace.clone(),
            audit_path.clone(),
        ) {
            Ok(proxy) => {
                let proxy_url = proxy.url();
                // Inject proxy env vars so tools route through our filter
                env_vars.push(("http_proxy".to_string(), proxy_url.clone()));
                env_vars.push(("https_proxy".to_string(), proxy_url.clone()));
                env_vars.push(("HTTP_PROXY".to_string(), proxy_url.clone()));
                env_vars.push(("HTTPS_PROXY".to_string(), proxy_url.clone()));
                // no_proxy for localhost — direct connections to localhost bypass proxy
                env_vars.push(("no_proxy".to_string(), "localhost,127.0.0.1,::1".to_string()));
                env_vars.push(("NO_PROXY".to_string(), "localhost,127.0.0.1,::1".to_string()));
                Some(proxy)
            }
            Err(e) => {
                eprintln!("[cato] warning: failed to start network proxy: {}", e);
                eprintln!("[cato]   network filtering disabled");
                None
            }
        }
    } else {
        None
    };

    // Generate platform-specific sandbox profile
    #[cfg(target_os = "macos")]
    let profile_path = {
        let profile_content = sandbox::seatbelt::generate(&resolved, &tool_paths);
        let path = std::env::temp_dir().join(format!("cato-{}.sb", std::process::id()));
        std::fs::write(&path, &profile_content).unwrap_or_else(|e| {
            eprintln!("[cato] Failed to write sandbox profile: {}", e);
            std::process::exit(1);
        });
        if std::env::var("CATO_DEBUG").is_ok() {
            eprintln!("[cato] Generated Seatbelt profile:");
            eprintln!("{}", profile_content);
            eprintln!("[cato] Profile path: {}", path.display());
        }
        path
    };

    #[cfg(not(target_os = "macos"))]
    let profile_path = std::env::temp_dir().join(format!("cato-{}.profile", std::process::id()));

    // Suppress unused warning for tool_paths on Linux
    #[cfg(not(target_os = "macos"))]
    let _ = &tool_paths;

    // Record start time and command for audit
    let start_time = std::time::Instant::now();
    let cmd_label = command.as_ref()
        .map(|c| {
            let full = c.join(" ");
            // Truncate long commands (multi-line scripts) for the audit log
            let first_line = full.lines().next().unwrap_or(&full);
            if first_line.len() > 120 {
                format!("{}...", &first_line[..120])
            } else if full.lines().count() > 1 {
                format!("{}...", first_line)
            } else {
                full
            }
        })
        .unwrap_or_else(|| "interactive".to_string());

    // Log sandbox start
    log_sandbox_event("sandbox_start", &resolved, Some(&cmd_label), None, None);

    // Print summary
    eprintln!("[cato] Sandbox active");
    eprintln!("[cato]   Workspace: {}", resolved.workspace);
    if !resolved.deny_read.is_empty() {
        eprintln!("[cato]   Read deny: {} patterns", resolved.deny_read.len());
    }
    if !resolved.deny_write.is_empty() {
        eprintln!("[cato]   Write deny: {} patterns", resolved.deny_write.len());
    }
    if resolved.network.iter().any(|d| d == "*") {
        eprintln!("[cato]   Network:   unrestricted");
    } else if !resolved.network.is_empty() {
        let proxy_info = _proxy.as_ref()
            .map(|p| format!(" (proxy :{})", p.port()))
            .unwrap_or_default();
        eprintln!("[cato]   Network:   {} allowed, deny all others{}",
            resolved.network.len(), proxy_info);
    } else {
        eprintln!("[cato]   Network:   blocked (no domains configured)");
    }
    if !env_vars.is_empty() {
        let secret_count = env_vars.iter()
            .filter(|(k, _)| !k.contains("proxy") && !k.contains("PROXY"))
            .count();
        if secret_count > 0 {
            eprintln!("[cato]   Secrets:   {} injected", secret_count);
        }
    }

    if resolved.options.ssh_agent {
        if std::env::var("SSH_AUTH_SOCK").is_ok() {
            eprintln!("[cato]   \x1b[33mSSH agent forwarded — processes can use your SSH keys (git push, ssh)\x1b[0m");
        }
    }

    if command.is_some() {
        eprintln!("[cato] Running command...");
    } else {
        eprintln!("[cato] Entering sandbox... (type 'exit' to leave)");
    }

    // Set up signal handler for graceful cleanup
    let profile_path_clone = profile_path.clone();
    let ephemeral_flag = ephemeral;
    ctrlc::set_handler(move || {
        // Cleanup on SIGINT/SIGTERM/SIGHUP
        let _ = std::fs::remove_file(&profile_path_clone);
        if ephemeral_flag {
            let tmp = std::env::temp_dir().join(format!("cato-eph-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&tmp);
        }
        // Can't call log_sandbox_event here (not enough context), but at least clean up files
        eprintln!("\n[cato] Sandbox interrupted.");
        std::process::exit(130);
    }).ok();

    // Run the sandbox
    let exit_code = sandbox::runner::run(&resolved, &profile_path, command, env_vars);
    let duration_secs = start_time.elapsed().as_secs();

    // Cleanup
    let _ = std::fs::remove_file(&profile_path);
    if ephemeral {
        let tmp = std::env::temp_dir().join(format!("cato-eph-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        eprintln!("[cato] Ephemeral workspace cleaned up.");
    }

    log_sandbox_event("sandbox_stop", &resolved, Some(&cmd_label), Some(exit_code), Some(duration_secs));
    eprintln!("[cato] Sandbox session ended.");

    std::process::exit(exit_code);
}

fn check_tools(tools: &[String]) -> Vec<String> {
    let store = crate::commands::tool::load_store_tools();
    let mut missing = Vec::new();

    for tool in tools {
        // Check global store first
        if let Some(path) = store.get(tool) {
            if std::path::Path::new(path).exists() {
                continue;
            }
        }
        // Try which
        if let Ok(output) = std::process::Command::new("which").arg(tool).output() {
            if output.status.success() {
                continue;
            }
        }
        missing.push(tool.clone());
    }
    missing
}

fn resolve_secrets(config: &sandbox::config::SandboxConfig) -> Vec<(String, String)> {
    let mut env_vars = Vec::new();

    if let Some(secrets_table) = config.secrets.as_table() {
        for (name, value) in secrets_table {
            // Try to resolve the secret value
            if let Some(resolved) = crate::commands::secret::resolve(name) {
                env_vars.push((name.clone(), resolved));
            } else if let Some(default) = value.get("default").and_then(|d| d.as_str()) {
                env_vars.push((name.clone(), default.to_string()));
            } else {
                eprintln!("[cato] \x1b[33mwarning:\x1b[0m secret {} not found (set via env var or `cato secret put {}`)", name, name);
            }
        }
    }

    env_vars
}

fn log_sandbox_event(
    event: &str,
    config: &sandbox::config::ResolvedConfig,
    command: Option<&str>,
    exit_code: Option<i32>,
    duration_secs: Option<u64>,
) {
    let mut entry = serde_json::json!({
        "ts": chrono::Utc::now().to_rfc3339(),
        "event": event,
        "workspace": config.workspace,
        "deny_read": config.deny_read.len(),
        "deny_write": config.deny_write.len(),
        "network": config.network.len(),
    });

    if let Some(cmd) = command {
        entry["command"] = serde_json::Value::String(cmd.to_string());
    }
    if let Some(code) = exit_code {
        entry["exit_code"] = serde_json::Value::Number(code.into());
    }
    if let Some(dur) = duration_secs {
        entry["duration_secs"] = serde_json::Value::Number(dur.into());
    }

    let log_path = crate::audit::log_path();

    if let Ok(json) = serde_json::to_string(&entry) {
        if let Ok(mut f) = crate::audit::open_append_private(&log_path) {
            use std::io::Write;
            let _ = writeln!(f, "{}", json);
        }
    }
}

fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            // Skip .git for speed in ephemeral mode
            if entry.file_name() == ".git" { continue; }
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}
