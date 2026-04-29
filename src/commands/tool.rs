use std::collections::BTreeMap;
use std::path::PathBuf;

fn store_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".cato")
        .join("store.toml")
}

fn load_store() -> toml::Value {
    let path = store_path();
    if path.exists() {
        let content = std::fs::read_to_string(&path).unwrap_or_default();
        toml::from_str(&content).unwrap_or_else(|_| default_store())
    } else {
        default_store()
    }
}

fn default_store() -> toml::Value {
    toml::Value::Table({
        let mut t = toml::map::Map::new();
        t.insert("tools".to_string(), toml::Value::Table(toml::map::Map::new()));
        t.insert("secrets".to_string(), toml::Value::Table(toml::map::Map::new()));
        t
    })
}

fn save_store(store: &toml::Value) {
    let path = store_path();
    let content = toml::to_string_pretty(store).unwrap_or_default();
    crate::audit::write_private(&path, &content).unwrap_or_else(|e| {
        eprintln!("[cato] Failed to save store: {}", e);
    });
}

fn get_tools(store: &toml::Value) -> BTreeMap<String, String> {
    store.get("tools")
        .and_then(|t| t.as_table())
        .map(|t| t.iter().filter_map(|(k, v)| {
            v.as_str().map(|s| (k.clone(), s.to_string()))
        }).collect())
        .unwrap_or_default()
}

/// Load registered tools from the global store
pub fn load_store_tools() -> std::collections::BTreeMap<String, String> {
    get_tools(&load_store())
}

pub fn add(name: &str, path: Option<&str>) {
    let resolved = if let Some(p) = path {
        let pb = PathBuf::from(p);
        if !pb.exists() {
            eprintln!("[cato] \x1b[31m✗\x1b[0m {} not found at {}", name, p);
            std::process::exit(1);
        }
        pb.canonicalize().unwrap_or(pb).to_string_lossy().to_string()
    } else {
        // Find via which
        match std::process::Command::new("which").arg(name).output() {
            Ok(output) if output.status.success() => {
                let p = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if p.is_empty() {
                    eprintln!("[cato] \x1b[31m✗\x1b[0m {} not found in PATH", name);
                    eprintln!("  Install it or use: cato tool add {} --path /path/to/{}", name, name);
                    std::process::exit(1);
                }
                p
            }
            _ => {
                eprintln!("[cato] \x1b[31m✗\x1b[0m {} not found in PATH", name);
                eprintln!("  Install it or use: cato tool add {} --path /path/to/{}", name, name);
                std::process::exit(1);
            }
        }
    };

    let mut store = load_store();
    if let Some(tools) = store.get_mut("tools").and_then(|t| t.as_table_mut()) {
        let existed = tools.contains_key(name);
        tools.insert(name.to_string(), toml::Value::String(resolved.clone()));
        save_store(&store);
        if existed {
            println!("[cato] \x1b[32m✓\x1b[0m {} → {} (updated)", name, resolved);
        } else {
            println!("[cato] \x1b[32m✓\x1b[0m {} → {}", name, resolved);
        }
    }

    // Auto-detect config directories for this tool
    let detected_configs = detect_tool_config(name);
    for config_dir in &detected_configs {
        println!("[cato] \x1b[32m✓\x1b[0m found config: {} (will mount read-only)", config_dir);
    }

    if detected_configs.is_empty() {
        println!("[cato]   no config directory detected for {}", name);
    }

    // Auto-add to project .cato.toml if it exists
    let cwd = std::env::current_dir().unwrap_or_default();
    let config_path = cwd.join(".cato.toml");
    if config_path.exists() {
        if let Ok(mut content) = std::fs::read_to_string(&config_path) {
            // Add tool to tools list
            let tool_quoted = format!("\"{}\"", name);
            if !content.contains(&tool_quoted) {
                if let Some(pos) = content.find("tools = [") {
                    if let Some(bracket_end) = content[pos..].find(']') {
                        let insert_pos = pos + bracket_end;
                        let before = &content[..insert_pos];
                        let after = &content[insert_pos..];
                        let list_content = &content[pos + 9..insert_pos];
                        content = if list_content.trim().is_empty() {
                            format!("{}\n    \"{}\",\n{}", before, name, after)
                        } else {
                            format!("{}\n    \"{}\",{}", before, name, after)
                        };
                        let _ = std::fs::write(&config_path, &content);
                        println!("[cato] \x1b[32m✓\x1b[0m added to .cato.toml tools list");
                    }
                }
            }

            // Add detected configs to allow_read
            // Re-read in case we just wrote
            if let Ok(content) = std::fs::read_to_string(&config_path) {
                let mut modified = content.clone();
                for config_dir in &detected_configs {
                    let quoted = format!("\"{}\"", config_dir);
                    if modified.contains(&quoted) { continue; }

                    if let Some(pos) = modified.find("allow_read = [") {
                        if let Some(bracket_end) = modified[pos..].find(']') {
                            let insert_pos = pos + bracket_end;
                            let before = &modified[..insert_pos];
                            let after = &modified[insert_pos..];
                            modified = format!("{}\n    \"{}\",{}", before, config_dir, after);
                        }
                    } else {
                        // No allow_read field yet — add it after [sandbox]
                        if let Some(pos) = modified.find("allow_write") {
                            let before = &modified[..pos];
                            let after = &modified[pos..];
                            modified = format!("{}allow_read = [\n    \"{}\",\n]\n\n{}", before, config_dir, after);
                        }
                    }
                }
                if modified != content {
                    let _ = std::fs::write(&config_path, &modified);
                    println!("[cato] \x1b[32m✓\x1b[0m added config paths to .cato.toml allow_read");
                }
            }
        }
    }

    // Hint about network
    if !detected_configs.is_empty() {
        println!("[cato]   hint: if {} needs network access, add its API domain to network in .cato.toml", name);
    }
}

/// Auto-detect config directories for a tool.
/// Scans common locations: ~/.<name>, ~/.config/<name>, ~/Library/Application Support/<name>
fn detect_tool_config(name: &str) -> Vec<String> {
    let mut found = Vec::new();
    if let Some(home) = dirs::home_dir() {
        let lower = name.to_lowercase();
        let candidates = vec![
            home.join(format!(".{}", name)),
            home.join(format!(".{}", lower)),
            home.join(".config").join(name),
            home.join(".config").join(&lower),
            // XDG data/state dirs
            home.join(".local/share").join(name),
            home.join(".local/share").join(&lower),
            home.join(".local/state").join(name),
            home.join(".local/state").join(&lower),
            // macOS Application Support
            home.join("Library/Application Support").join(name),
            home.join("Library/Application Support").join(capitalize(name)),
        ];

        let home_str = home.to_string_lossy().to_string();

        for path in candidates {
            if path.exists() && path.is_dir() {
                let display = path.to_string_lossy().replace(&home_str, "~");
                if !found.contains(&display) {
                    found.push(display);
                }
            }
        }

        // Also check for dotfiles in home root (e.g., ~/.claude.json)
        for ext in &["json", "toml", "yaml", "yml", "conf", "cfg"] {
            let dotfile = home.join(format!(".{}.{}", lower, ext));
            if dotfile.exists() && dotfile.is_file() {
                let display = dotfile.to_string_lossy().replace(&home_str, "~");
                if !found.contains(&display) {
                    found.push(display);
                }
            }
        }
    }
    found
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

pub fn list() {
    let store = load_store();
    let tools = get_tools(&store);

    if tools.is_empty() {
        println!("[cato] No tools registered.");
        println!("  Use: cato tool add <name>");
        return;
    }

    println!("Registered tools:");
    for (name, path) in &tools {
        let exists = std::path::Path::new(path).exists();
        let icon = if exists { "\x1b[32m✓\x1b[0m" } else { "\x1b[31m✗\x1b[0m" };
        println!("  {} {:<15} → {}{}", icon, name, path,
            if !exists { " (missing!)" } else { "" });
    }
}

pub fn remove(name: &str) {
    let mut store = load_store();
    if let Some(tools) = store.get_mut("tools").and_then(|t| t.as_table_mut()) {
        if tools.remove(name).is_some() {
            save_store(&store);
            println!("[cato] \x1b[32m✓\x1b[0m {} removed", name);
        } else {
            println!("[cato] {} not found in registered tools", name);
        }
    }
}
