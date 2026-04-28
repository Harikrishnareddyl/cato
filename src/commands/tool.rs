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
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let content = toml::to_string_pretty(store).unwrap_or_default();
    std::fs::write(&path, content).unwrap_or_else(|e| {
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

    // Auto-add to project .cato.toml if it exists
    let cwd = std::env::current_dir().unwrap_or_default();
    let config_path = cwd.join(".cato.toml");
    if config_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&config_path) {
            // Check if tool is already in the tools list
            let tool_quoted = format!("\"{}\"", name);
            if !content.contains(&tool_quoted) {
                // Find the tools = [...] line and add the tool
                if let Some(pos) = content.find("tools = [") {
                    if let Some(bracket_end) = content[pos..].find(']') {
                        let insert_pos = pos + bracket_end;
                        let before = &content[..insert_pos];
                        let after = &content[insert_pos..];
                        // Check if list is empty or has items
                        let list_content = &content[pos + 9..insert_pos];
                        let new_content = if list_content.trim().is_empty() {
                            format!("{}\n    \"{}\",\n{}", before, name, after)
                        } else {
                            format!("{}\n    \"{}\",{}", before, name, after)
                        };
                        let _ = std::fs::write(&config_path, new_content);
                        println!("[cato] \x1b[32m✓\x1b[0m Added to .cato.toml tools list");
                    }
                }
            }
        }
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
