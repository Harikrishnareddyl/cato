use std::path::Path;

pub fn run(minimal: bool, strict: bool, force: bool) {
    let cwd = std::env::current_dir().unwrap_or_else(|_| ".".into());
    let config_path = cwd.join(".cato.toml");

    // Check existing config
    if config_path.exists() {
        if force {
            println!("[cato] Overwriting existing .cato.toml (--force)");
        } else {
            println!("[cato] .cato.toml already exists.");
            println!("  Use --force to overwrite, or edit it manually.");
            return;
        }
    }

    // Auto-detect project
    let detected = detect_project(&cwd);

    // Generate config
    let level = if minimal { "minimal" } else if strict { "strict" } else { "default" };
    let content = generate_config(level, &detected);

    std::fs::write(&config_path, &content).expect("Failed to write .cato.toml");

    // Ensure global cato dir exists with restricted permissions
    crate::audit::ensure_cato_dir();

    // Output
    println!("[cato] Created .cato.toml ({} protection)", level);
    println!();
    println!("  Detected:");
    if !detected.network.is_empty() {
        println!("    Network: {}", detected.network.join(", "));
    }
    if !detected.tools.is_empty() {
        println!("    Tools:   {}", detected.tools.join(", "));
    }
    println!();
    println!("  Sandbox denies:");
    println!("    Read:    *.env, *.pem, *.key, ~/.ssh/*, ~/.aws/*");
    println!("    Write:   everything outside workspace");
    println!("    Network: everything not in allowlist");
    println!();
    println!("  Next steps:");
    println!("    cato tool add <name>       Register tools for sandbox");
    println!("    cato secret put <NAME>     Store secrets for sandbox");
    println!("    cato run                   Enter sandbox");
    println!();
    println!("  Commit .cato.toml to git for team-wide rules.");
}

struct ProjectDetection {
    network: Vec<String>,
    tools: Vec<String>,
}

fn detect_project(cwd: &Path) -> ProjectDetection {
    let mut network = Vec::new();
    let mut tools = Vec::new();

    // Detect git remotes
    if cwd.join(".git").exists() {
        tools.push("git".to_string());
        if let Ok(config) = std::fs::read_to_string(cwd.join(".git/config")) {
            if config.contains("github.com") { network.push("github.com".to_string()); }
            if config.contains("gitlab.com") { network.push("gitlab.com".to_string()); }
            if config.contains("bitbucket.org") { network.push("bitbucket.org".to_string()); }
        }
    }

    // Detect Node.js
    if cwd.join("package.json").exists() {
        tools.push("node".to_string());
        tools.push("npm".to_string());
        network.push("registry.npmjs.org".to_string());
    }

    // Detect Python
    if cwd.join("requirements.txt").exists()
        || cwd.join("Pipfile").exists()
        || cwd.join("pyproject.toml").exists()
        || cwd.join("setup.py").exists()
    {
        tools.push("python3".to_string());
        network.push("pypi.org".to_string());
        network.push("files.pythonhosted.org".to_string());
    }

    // Detect Rust
    if cwd.join("Cargo.toml").exists() {
        tools.push("cargo".to_string());
        network.push("crates.io".to_string());
        network.push("static.crates.io".to_string());
    }

    // Detect Go
    if cwd.join("go.mod").exists() {
        tools.push("go".to_string());
        network.push("proxy.golang.org".to_string());
    }

    // Detect Terraform
    if cwd.join("main.tf").exists() || cwd.join("terraform.tf").exists() {
        tools.push("terraform".to_string());
        network.push("registry.terraform.io".to_string());
    }

    // Detect Docker
    if cwd.join("Dockerfile").exists() || cwd.join("docker-compose.yml").exists() {
        tools.push("docker".to_string());
    }

    // Deduplicate
    network.sort();
    network.dedup();
    tools.sort();
    tools.dedup();

    ProjectDetection { network, tools }
}

fn generate_config(level: &str, detected: &ProjectDetection) -> String {
    let mut lines = Vec::new();

    lines.push("# Cato Sandbox Configuration".to_string());
    lines.push("# https://github.com/Harikrishnareddyl/cato".to_string());
    lines.push("# Commit this file to git — shared sandbox rules for the team.".to_string());
    lines.push(String::new());

    // Sandbox section
    lines.push("[sandbox]".to_string());
    lines.push("# Write access: deny by default. Only these paths are writable.".to_string());
    lines.push("allow_write = [\"{workspace}\", \"/tmp\"]".to_string());
    lines.push(String::new());

    // deny_write
    lines.push("# Write deny: block writes to these patterns even within allow_write paths".to_string());
    lines.push("deny_write = [".to_string());
    lines.push("    \"*.lock\",".to_string());
    if level == "strict" {
        lines.push("    \".github/*\",".to_string());
        lines.push("    \"migrations/*\",".to_string());
    }
    lines.push("]".to_string());
    lines.push(String::new());

    // deny_read
    lines.push("# Read deny: block reads for these patterns (kernel-enforced)".to_string());
    lines.push("deny_read = [".to_string());
    lines.push("    \"*.env\",".to_string());
    lines.push("    \"*.env.*\",".to_string());
    lines.push("    \"*.pem\",".to_string());
    lines.push("    \"*.key\",".to_string());
    lines.push("    \"*.p12\",".to_string());
    lines.push("    \"id_rsa\",".to_string());
    lines.push("    \"id_ed25519\",".to_string());
    if level != "minimal" {
        lines.push("    \"*credentials*\",".to_string());
        lines.push("    \"*.keystore\",".to_string());
        lines.push("    \".git-credentials\",".to_string());
    }
    lines.push("]".to_string());
    lines.push(String::new());

    // Network — deny by default, list trusted sources
    lines.push("# Network: deny by default. Only listed domains are reachable.".to_string());
    lines.push("# Use [\"*\"] for unrestricted network access.".to_string());
    lines.push("network = [".to_string());
    // Always include detected domains
    for domain in &detected.network {
        lines.push(format!("    \"{}\",", domain));
    }
    // Add common trusted sources
    if !detected.network.iter().any(|d| d.contains("github")) {
        lines.push("    \"github.com\",".to_string());
    }
    if level != "minimal" {
        if !detected.network.iter().any(|d| d.contains("npmjs")) {
            if detected.tools.iter().any(|t| t == "node" || t == "npm") {
                lines.push("    \"registry.npmjs.org\",".to_string());
            }
        }
        if !detected.network.iter().any(|d| d.contains("pypi")) {
            if detected.tools.iter().any(|t| t == "python3" || t == "pip") {
                lines.push("    \"pypi.org\",".to_string());
                lines.push("    \"files.pythonhosted.org\",".to_string());
            }
        }
    }
    lines.push("]".to_string());
    lines.push(String::new());

    // Tools
    lines.push("tools = [".to_string());
    for tool in &detected.tools {
        lines.push(format!("    \"{}\",", tool));
    }
    lines.push("]".to_string());
    lines.push(String::new());

    // Secrets
    lines.push("[sandbox.secrets]".to_string());
    lines.push("# Declare secrets needed inside the sandbox.".to_string());
    lines.push("# Values come from: host env vars → ~/.cato/store.toml → error".to_string());
    lines.push("# ANTHROPIC_API_KEY = {}".to_string());
    lines.push("# DATABASE_URL = { default = \"postgres://localhost/mydb\" }".to_string());
    lines.push(String::new());

    // Options
    lines.push("[sandbox.options]".to_string());
    lines.push("# ssh_agent = true  # enable if you need git push via SSH (forwards your keys)".to_string());
    lines.push("allow_localhost = true".to_string());
    lines.push("# log_level = \"normal\"  # quiet(0) | normal(1) | verbose(2) | debug(3)".to_string());
    lines.push("# Override with env var: CATO_LOG=verbose cato run ...".to_string());

    // Strict additions
    if level == "strict" {
        lines.push(String::new());
        lines.push("# Strict mode: additional restrictions".to_string());
        lines.push("[sandbox.strict]".to_string());
        lines.push("deny_write_config = [\".github/workflows/*\", \"Dockerfile\", \"docker-compose.yml\", \"*.tf\"]".to_string());
    }

    lines.push(String::new());
    lines.join("\n")
}
