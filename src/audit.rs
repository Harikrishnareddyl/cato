use std::path::PathBuf;

/// Get the audit log path.
pub fn log_path() -> PathBuf {
    cato_dir().join("audit.jsonl")
}

/// Get the ~/.cato directory path.
pub fn cato_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".cato")
}

/// Ensure ~/.cato/ exists with 0700 permissions.
pub fn ensure_cato_dir() {
    let dir = cato_dir();
    if !dir.exists() {
        let _ = std::fs::create_dir_all(&dir);
    }
    // Set directory to owner-only (0700)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
    }
}

/// Write a file with 0600 permissions (owner read/write only).
pub fn write_private(path: &std::path::Path, content: &str) -> std::io::Result<()> {
    ensure_cato_dir();
    std::fs::write(path, content)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

/// Open a file for appending with 0600 permissions.
pub fn open_append_private(path: &std::path::Path) -> std::io::Result<std::fs::File> {
    ensure_cato_dir();
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(file)
}
