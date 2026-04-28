use crate::audit::log_path;
use std::io::BufRead;

pub fn run(count: usize, follow: bool, filter: Option<&str>, all: bool) {
    let path = log_path();
    if !path.exists() {
        println!("No audit log found at {}", path.display());
        println!("Logs appear after the first sandbox session.");
        return;
    }

    // Resolve current workspace for filtering
    let workspace_match = if all {
        None
    } else {
        let cwd = std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        let canonical = std::fs::canonicalize(&cwd)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        Some((cwd, canonical))
    };

    let content = std::fs::read_to_string(&path).unwrap_or_default();
    let lines: Vec<&str> = content.lines().collect();

    // Filter by workspace (parse JSON, compare workspace field), then by keyword
    let filtered: Vec<&str> = lines.iter()
        .filter(|l| match &workspace_match {
            Some((cwd, canonical)) => entry_matches_workspace(l, cwd, canonical),
            None => true,
        })
        .filter(|l| match filter {
            Some(f) => l.contains(f),
            None => true,
        })
        .cloned()
        .collect();

    let start = if filtered.len() > count { filtered.len() - count } else { 0 };

    // Header
    if let Some((ref cwd, _)) = workspace_match {
        let short = std::path::Path::new(cwd)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| cwd.clone());
        println!("Project: {} (use --all for all projects)", short);
        println!();
    }

    println!("{:<20} {:<18} {}",
        "TIMESTAMP", "EVENT", "DETAILS");
    println!("{}", "-".repeat(70));

    if filtered[start..].is_empty() {
        println!("  (no entries)");
    }

    for line in &filtered[start..] {
        print_entry(line);
    }

    if follow {
        println!("{}", "-".repeat(80));
        println!("Following... (Ctrl+C to stop)");

        let mut last_size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        loop {
            std::thread::sleep(std::time::Duration::from_secs(1));
            let current_size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            if current_size > last_size {
                if let Ok(f) = std::fs::File::open(&path) {
                    use std::io::Seek;
                    let mut reader = std::io::BufReader::new(f);
                    reader.seek(std::io::SeekFrom::Start(last_size)).ok();
                    for line in reader.lines().flatten() {
                        let ws_ok = match &workspace_match {
                            Some((cwd, canonical)) => entry_matches_workspace(&line, cwd, canonical),
                            None => true,
                        };
                        let filter_ok = match filter {
                            Some(f) => line.contains(f),
                            None => true,
                        };
                        if ws_ok && filter_ok {
                            print_entry(&line);
                        }
                    }
                }
                last_size = current_size;
            }
        }
    }
}

/// Check if a JSON log entry's workspace field matches the given paths
fn entry_matches_workspace(line: &str, cwd: &str, canonical: &str) -> bool {
    if let Ok(entry) = serde_json::from_str::<serde_json::Value>(line) {
        if let Some(ws) = entry.get("workspace").and_then(|v| v.as_str()) {
            return ws == cwd || ws == canonical;
        }
    }
    false
}

fn print_entry(line: &str) {
    if let Ok(entry) = serde_json::from_str::<serde_json::Value>(line) {
        let ts = entry.get("ts").and_then(|v| v.as_str()).unwrap_or("-");
        let event = entry.get("event").and_then(|v| v.as_str()).unwrap_or("-");

        let short_ts = if ts.len() > 19 { &ts[..19] } else { ts };

        let (icon, details) = match event {
            "sandbox_start" => {
                let cmd = entry.get("command").and_then(|v| v.as_str()).unwrap_or("-");
                let short_cmd = cmd.lines().next().unwrap_or(cmd);
                let display = if short_cmd.len() > 60 {
                    format!("{}...", &short_cmd[..60])
                } else {
                    short_cmd.to_string()
                };
                ("\x1b[32m+\x1b[0m", format!("cmd: {}", display))
            }
            "sandbox_stop" => {
                let exit = entry.get("exit_code").and_then(|v| v.as_i64()).unwrap_or(-1);
                let dur = entry.get("duration_secs").and_then(|v| v.as_u64()).unwrap_or(0);
                let dur_str = if dur >= 3600 {
                    format!("{}h{}m", dur / 3600, (dur % 3600) / 60)
                } else if dur >= 60 {
                    format!("{}m{}s", dur / 60, dur % 60)
                } else {
                    format!("{}s", dur)
                };
                let exit_icon = if exit == 0 { "\x1b[32mok\x1b[0m" } else { &format!("\x1b[31mexit {}\x1b[0m", exit) };
                ("\x1b[33m-\x1b[0m", format!("{} ({})", exit_icon, dur_str))
            }
            "network_denied" => {
                let domain = entry.get("domain").and_then(|v| v.as_str()).unwrap_or("?");
                ("\x1b[31mx\x1b[0m", format!("blocked: {}", domain))
            }
            _ => (" ", "-".to_string()),
        };

        println!("{} {:<19} {:<18} {}",
            icon, short_ts.replace("T", " "), event, details);
    }
}
