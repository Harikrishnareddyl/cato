use std::path::PathBuf;
use std::io::{self, Write};

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

fn project_key() -> Option<String> {
    std::env::current_dir()
        .ok()
        .map(|p| p.to_string_lossy().to_string())
}

pub fn put(name_value: &str, project: bool) {
    let (name, value) = if let Some(eq_pos) = name_value.find('=') {
        let n = &name_value[..eq_pos];
        let v = &name_value[eq_pos + 1..];
        (n.to_string(), v.to_string())
    } else {
        let name = name_value.to_string();
        eprint!("Enter value for {}: ", name);
        io::stderr().flush().ok();
        let mut value = String::new();
        io::stdin().read_line(&mut value).unwrap_or_default();
        let value = value.trim().to_string();
        if value.is_empty() {
            eprintln!("[cato] Empty value. Secret not saved.");
            std::process::exit(1);
        }
        (name, value)
    };

    let mut store = load_store();

    if project {
        let proj = project_key().unwrap_or_else(|| {
            eprintln!("[cato] Cannot determine project directory");
            std::process::exit(1);
        });

        // Ensure secrets.projects table exists
        let secrets = store
            .as_table_mut().unwrap()
            .entry("secrets")
            .or_insert(toml::Value::Table(toml::map::Map::new()))
            .as_table_mut().unwrap();

        let projects = secrets
            .entry("projects")
            .or_insert(toml::Value::Table(toml::map::Map::new()))
            .as_table_mut().unwrap();

        let proj_table = projects
            .entry(&proj)
            .or_insert(toml::Value::Table(toml::map::Map::new()))
            .as_table_mut().unwrap();

        let existed = proj_table.contains_key(&name);
        proj_table.insert(name.clone(), toml::Value::String(value));
        save_store(&store);

        let short_proj = proj.split('/').last().unwrap_or(&proj);
        if existed {
            println!("[cato] \x1b[32m✓\x1b[0m {} saved (project: {}, updated)", name, short_proj);
        } else {
            println!("[cato] \x1b[32m✓\x1b[0m {} saved (project: {})", name, short_proj);
        }
    } else {
        if let Some(secrets) = store.get_mut("secrets").and_then(|t| t.as_table_mut()) {
            let existed = secrets.contains_key(&name);
            secrets.insert(name.clone(), toml::Value::String(value));
            save_store(&store);
            if existed {
                println!("[cato] \x1b[32m✓\x1b[0m {} saved (global, updated)", name);
            } else {
                println!("[cato] \x1b[32m✓\x1b[0m {} saved (global)", name);
            }
        }
    }
}

pub fn list() {
    let store = load_store();
    let secrets = store.get("secrets")
        .and_then(|t| t.as_table())
        .cloned()
        .unwrap_or_default();

    let has_global = secrets.iter().any(|(k, v)| k != "projects" && v.is_str());
    let proj = project_key();
    let project_secrets = proj.as_ref().and_then(|p| {
        secrets.get("projects")
            .and_then(|t| t.as_table())
            .and_then(|t| t.get(p.as_str()))
            .and_then(|t| t.as_table())
            .cloned()
    });
    let has_project = project_secrets.as_ref().map(|t| !t.is_empty()).unwrap_or(false);

    if !has_global && !has_project {
        println!("[cato] No secrets stored.");
        println!("  Use: cato secret put <NAME>");
        return;
    }

    if has_global {
        println!("Global secrets:");
        for (name, value) in &secrets {
            if name == "projects" || !value.is_str() { continue; }
            println!("  {:<30} {}", name, mask(value.as_str().unwrap_or("")));
        }
    }

    if let Some(proj_secrets) = project_secrets {
        let short = proj.as_ref().and_then(|p| p.split('/').last()).unwrap_or("?");
        println!("Project secrets ({}):", short);
        for (name, value) in &proj_secrets {
            println!("  {:<30} {} \x1b[2m(overrides global)\x1b[0m", name, mask(value.as_str().unwrap_or("")));
        }
    }
}

pub fn remove(name: &str, project: bool) {
    let mut store = load_store();

    if project {
        let proj = project_key().unwrap_or_default();
        let removed = store.get_mut("secrets")
            .and_then(|s| s.as_table_mut())
            .and_then(|s| s.get_mut("projects"))
            .and_then(|p| p.as_table_mut())
            .and_then(|p| p.get_mut(&proj))
            .and_then(|t| t.as_table_mut())
            .and_then(|t| t.remove(name))
            .is_some();

        if removed {
            save_store(&store);
            println!("[cato] \x1b[32m✓\x1b[0m {} removed (project)", name);
        } else {
            println!("[cato] {} not found in project secrets", name);
        }
    } else {
        if let Some(secrets) = store.get_mut("secrets").and_then(|t| t.as_table_mut()) {
            if secrets.remove(name).is_some() {
                save_store(&store);
                println!("[cato] \x1b[32m✓\x1b[0m {} removed (global)", name);
            } else {
                println!("[cato] {} not found in global secrets", name);
            }
        }
    }
}

/// Resolve a secret value: host env → project scope → global scope → None
pub fn resolve(name: &str) -> Option<String> {
    // 1. Host environment variable
    if let Ok(val) = std::env::var(name) {
        return Some(val);
    }

    let store = load_store();
    let secrets = store.get("secrets").and_then(|t| t.as_table())?;

    // 2. Project-scoped override
    if let Some(proj) = project_key() {
        if let Some(val) = secrets.get("projects")
            .and_then(|t| t.as_table())
            .and_then(|t| t.get(&proj))
            .and_then(|t| t.as_table())
            .and_then(|t| t.get(name))
            .and_then(|v| v.as_str())
        {
            return Some(val.to_string());
        }
    }

    // 3. Global
    secrets.get(name).and_then(|v| v.as_str()).map(|s| s.to_string())
}

fn mask(val: &str) -> String {
    if val.len() > 6 {
        format!("{}...{}", &val[..3], &val[val.len()-3..])
    } else {
        "***".to_string()
    }
}
