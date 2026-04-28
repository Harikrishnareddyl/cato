use crate::audit;
use crate::sandbox;

const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn run() {
    let cwd = std::env::current_dir().unwrap_or_else(|_| ".".into());

    println!("Cato v{}", VERSION);
    println!();

    // Check if inside a sandbox
    if std::env::var("CATO_SANDBOX").is_ok() {
        println!("\x1b[33m🔒 Inside sandbox\x1b[0m");
        println!();
    }

    // Project config
    let config_path = cwd.join(".cato.toml");
    if config_path.exists() {
        println!("Project:    {}", cwd.display());
        match sandbox::config::load(&config_path) {
            Ok(config) => {
                // Writable paths
                println!("Writable:   {} paths", config.writable.len());

                // Deny read patterns
                if !config.deny_read.is_empty() {
                    println!("Deny read:  {} patterns", config.deny_read.len());
                }

                // Network
                if !config.network.is_empty() {
                    println!("Network:    {} allowed domains", config.network.len());
                    for domain in &config.network {
                        println!("              {}", domain);
                    }
                } else {
                    println!("Network:    unrestricted");
                }

                // Tools
                if !config.tools.is_empty() {
                    let store = crate::commands::tool::load_store_tools();
                    let mut ok = Vec::new();
                    let mut missing = Vec::new();
                    for tool in &config.tools {
                        if store.contains_key(tool) || which_exists(tool) {
                            ok.push(tool.as_str());
                        } else {
                            missing.push(tool.as_str());
                        }
                    }
                    if missing.is_empty() {
                        println!("Tools:      {} ready ({})", ok.len(), ok.join(", "));
                    } else {
                        println!("Tools:      {} ready, \x1b[31m{} missing\x1b[0m ({})",
                            ok.len(), missing.len(), missing.join(", "));
                    }
                }

                // Secrets
                if let Some(secrets_table) = config.secrets.as_table() {
                    if !secrets_table.is_empty() {
                        let mut resolved = 0;
                        let mut unresolved = Vec::new();
                        for (name, _) in secrets_table {
                            if crate::commands::secret::resolve(name).is_some() {
                                resolved += 1;
                            } else {
                                unresolved.push(name.as_str());
                            }
                        }
                        if unresolved.is_empty() {
                            println!("Secrets:    {} ready", resolved);
                        } else {
                            println!("Secrets:    {} ready, \x1b[33m{} missing\x1b[0m ({})",
                                resolved, unresolved.len(), unresolved.join(", "));
                        }
                    }
                }

                // Options
                if config.options.ssh_agent {
                    let sock = std::env::var("SSH_AUTH_SOCK").unwrap_or_default();
                    if sock.is_empty() {
                        println!("SSH agent:  \x1b[33menabled but SSH_AUTH_SOCK not set\x1b[0m");
                    } else {
                        println!("SSH agent:  ready");
                    }
                }
            }
            Err(e) => {
                println!("\x1b[31mConfig error: {}\x1b[0m", e);
            }
        }
    } else {
        println!("Project:    (no .cato.toml — run `cato init`)");
    }

    // Audit stats — filtered to current project by workspace field
    let log_path = audit::log_path();
    if log_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&log_path) {
            let cwd_str = cwd.to_string_lossy().to_string();
            let canonical = std::fs::canonicalize(&cwd)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();

            let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
            let mut sessions = 0usize;
            let mut today_sessions = 0usize;

            for line in content.lines() {
                if let Ok(entry) = serde_json::from_str::<serde_json::Value>(line) {
                    let ws = entry.get("workspace").and_then(|v| v.as_str()).unwrap_or("");
                    if ws != cwd_str && ws != canonical { continue; }
                    let event = entry.get("event").and_then(|v| v.as_str()).unwrap_or("");
                    if event == "sandbox_start" {
                        sessions += 1;
                        let ts = entry.get("ts").and_then(|v| v.as_str()).unwrap_or("");
                        if ts.starts_with(&today) {
                            today_sessions += 1;
                        }
                    }
                }
            }

            println!("Audit:      {} sessions ({} today)", sessions, today_sessions);
        }
    }

    println!();
    if config_path.exists() {
        println!("\x1b[32mReady — run `cato run` to enter sandbox\x1b[0m");
    } else {
        println!("Run `cato init` to set up this project");
    }
}

fn which_exists(name: &str) -> bool {
    std::process::Command::new("which")
        .arg(name)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}
