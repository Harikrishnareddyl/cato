use super::config::{self, ResolvedConfig};
use std::path::Path;

/// Run the sandbox
pub fn run(
    resolved: &ResolvedConfig,
    profile_path: &Path,
    command: Option<Vec<String>>,
    env_vars: Vec<(String, String)>,
) -> i32 {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());

    // Build the command
    let mut cmd = std::process::Command::new("/usr/bin/sandbox-exec");
    cmd.arg("-f").arg(profile_path);

    if let Some(ref args) = command {
        if let Some(first) = args.first() {
            cmd.arg(first);
            for arg in &args[1..] {
                cmd.arg(arg);
            }
        }
    } else {
        // Interactive shell
        cmd.arg(&shell);
    }

    // Clear environment and set only what we want
    cmd.env_clear();

    // System essentials
    cmd.env("HOME", std::env::var("HOME").unwrap_or_default());
    cmd.env("USER", std::env::var("USER").unwrap_or_default());
    cmd.env("SHELL", &shell);
    cmd.env("TERM", std::env::var("TERM").unwrap_or_else(|_| "xterm-256color".to_string()));
    cmd.env("LANG", std::env::var("LANG").unwrap_or_else(|_| "en_US.UTF-8".to_string()));
    cmd.env("PATH", std::env::var("PATH").unwrap_or_default());

    // Cato sandbox indicator
    cmd.env("CATO_SANDBOX", "1");

    // Custom prompt — create a wrapper zshrc that sources user's config then sets prompt
    let workspace_name = Path::new(&resolved.workspace)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "sandbox".to_string());
    cmd.env("CATO_WORKSPACE", &workspace_name);

    // Create a temp zdotdir with a .zshrc that sources user's then prepends lock icon
    let zdotdir = std::env::temp_dir().join(format!("cato-zsh-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&zdotdir);
    let zshrc_content = format!(
        "# Cato sandbox shell\n\
         [[ -f ~/.zshrc ]] && source ~/.zshrc 2>/dev/null\n\
         PROMPT='%F{{yellow}}🔒 {}%f %~ $ '\n",
        workspace_name
    );
    let _ = std::fs::write(zdotdir.join(".zshrc"), &zshrc_content);
    cmd.env("ZDOTDIR", &zdotdir);

    // Injected env vars (secrets)
    for (key, value) in &env_vars {
        cmd.env(key, value);
    }

    // SSH agent forwarding
    if resolved.options.ssh_agent {
        if let Ok(sock) = std::env::var("SSH_AUTH_SOCK") {
            cmd.env("SSH_AUTH_SOCK", sock);
        }
    }

    // Working directory
    cmd.current_dir(&resolved.workspace);

    // Execute — capture both stdout/stderr for debugging
    let debug = std::env::var("CATO_DEBUG").is_ok();
    if debug {
        eprintln!("[cato] Running: sandbox-exec -f {} -- {:?}", profile_path.display(),
            command.as_ref().map(|c| c.join(" ")).unwrap_or_else(|| shell.clone()));
    }

    let exit_code = if debug {
        // In debug mode, capture output to see errors
        let output = cmd.output();
        match output {
            Ok(output) => {
                // Forward stdout/stderr
                use std::io::Write;
                let _ = std::io::stdout().write_all(&output.stdout);
                let _ = std::io::stderr().write_all(&output.stderr);
                let code = output.status.code().unwrap_or(1);
                eprintln!("[cato] sandbox-exec exited with code: {}", code);
                if !output.stderr.is_empty() {
                    eprintln!("[cato] stderr: {}", String::from_utf8_lossy(&output.stderr));
                }
                code
            }
            Err(e) => {
                eprintln!("[cato] Failed to start sandbox: {}", e);
                1
            }
        }
    } else {
        match cmd.status() {
            Ok(status) => status.code().unwrap_or(1),
            Err(e) => {
                eprintln!("[cato] Failed to start sandbox: {}", e);
                if e.kind() == std::io::ErrorKind::NotFound {
                    eprintln!("[cato] sandbox-exec not found. This requires macOS.");
                }
                1
            }
        }
    };

    // Cleanup temp zdotdir
    let _ = std::fs::remove_dir_all(&zdotdir);

    exit_code
}
