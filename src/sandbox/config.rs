use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct SandboxConfig {
    #[serde(default = "default_writable")]
    pub writable: Vec<String>,

    #[serde(default)]
    pub deny_read: Vec<String>,

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
}

fn default_writable() -> Vec<String> {
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

    let writable: Vec<String> = config.writable.iter()
        .map(|p| resolve_path(p, &workspace_str, &home))
        .collect();

    let deny_read: Vec<String> = config.deny_read.iter()
        .map(|p| resolve_path(p, &workspace_str, &home))
        .collect();

    ResolvedConfig {
        workspace: workspace_str,
        writable,
        deny_read,
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
    pub writable: Vec<String>,
    pub deny_read: Vec<String>,
    pub network: Vec<String>,
    pub tools: Vec<String>,
    pub options: SandboxOptions,
}
