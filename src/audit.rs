use std::path::PathBuf;

/// Get the audit log path.
pub fn log_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".cato")
        .join("audit.jsonl")
}
