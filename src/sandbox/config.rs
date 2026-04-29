use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct SandboxConfig {
    /// Paths where writes are allowed (deny-by-default everywhere else)
    #[serde(default = "default_allow_write", alias = "writable")]
    pub allow_write: Vec<String>,

    /// Patterns blocked from writing even within allow_write paths (deny overrides allow)
    #[serde(default)]
    pub deny_write: Vec<String>,

    /// Patterns blocked from reading (deny overrides default allow-all reads)
    #[serde(default)]
    pub deny_read: Vec<String>,

    /// Host directories mounted read-only into the sandbox
    /// (e.g., ~/.claude for tool auth configs)
    #[serde(default)]
    pub allow_read: Vec<String>,

    /// Allowed network domains (deny-by-default, empty = no outbound)
    /// Use ["*"] for unrestricted network access
    #[serde(default)]
    pub network: Vec<String>,

    #[serde(default)]
    pub tools: Vec<String>,

    #[serde(default = "empty_table")]
    pub secrets: toml::Value,

    #[serde(default)]
    pub options: SandboxOptions,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct SandboxOptions {
    #[serde(default)]
    pub ssh_agent: bool,
    #[serde(default = "default_true")]
    pub allow_localhost: bool,
    /// Log level: quiet(0), normal(1), verbose(2), debug(3)
    #[serde(default)]
    pub log_level: Option<String>,
}

fn default_allow_write() -> Vec<String> {
    vec!["{workspace}".to_string(), "/tmp".to_string()]
}

fn default_true() -> bool {
    true
}

fn empty_table() -> toml::Value {
    toml::Value::Table(toml::map::Map::new())
}

#[derive(Debug, Deserialize)]
struct ConfigFile {
    sandbox: Option<SandboxConfig>,
}

/// Load sandbox config from .cato.toml
pub fn load(path: &Path) -> Result<SandboxConfig, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Cannot read {}: {}", path.display(), e))?;

    let config: ConfigFile = toml::from_str(&content)
        .map_err(|e| format!("Cannot parse {}: {}", path.display(), e))?;

    config.sandbox.ok_or_else(|| {
        format!("No [sandbox] section in {}. Run `cato init` first.", path.display())
    })
}

/// Resolve tokens and expand paths in the config
pub fn resolve(config: &SandboxConfig, workspace: &Path) -> ResolvedConfig {
    let workspace_str = workspace.to_string_lossy().to_string();
    let home = dirs::home_dir()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|| "/tmp".to_string());

    let allow_write: Vec<String> = config.allow_write.iter()
        .map(|p| resolve_path(p, &workspace_str, &home))
        .collect();

    let deny_write: Vec<String> = config.deny_write.iter()
        .map(|p| resolve_path(p, &workspace_str, &home))
        .collect();

    let deny_read: Vec<String> = config.deny_read.iter()
        .map(|p| resolve_path(p, &workspace_str, &home))
        .collect();

    let allow_read: Vec<String> = config.allow_read.iter()
        .map(|p| resolve_path(p, &workspace_str, &home))
        .collect();

    ResolvedConfig {
        workspace: workspace_str,
        allow_write,
        deny_write,
        deny_read,
        allow_read,
        network: config.network.clone(),
        tools: config.tools.clone(),
        options: config.options.clone(),
    }
}

fn resolve_path(path: &str, workspace: &str, home: &str) -> String {
    path.replace("{workspace}", workspace)
        .replace("~", home)
}

/// Fully resolved sandbox config with absolute paths
#[derive(Debug)]
pub struct ResolvedConfig {
    pub workspace: String,
    pub allow_write: Vec<String>,
    pub deny_write: Vec<String>,
    pub deny_read: Vec<String>,
    pub allow_read: Vec<String>,
    pub network: Vec<String>,
    pub tools: Vec<String>,
    pub options: SandboxOptions,
}
