use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;

/// A minimal HTTP CONNECT proxy that only allows connections to specified domains.
/// Runs on localhost, used inside the sandbox via http_proxy/https_proxy env vars.
/// All outbound from sandbox goes through this — domains not in the allow list are rejected.

pub struct NetworkProxy {
    port: u16,
    handle: Option<thread::JoinHandle<()>>,
}

/// Shared context passed to proxy handler threads
struct ProxyContext {
    domains: Vec<String>,
    workspace: String,
    audit_path: PathBuf,
}

impl NetworkProxy {
    /// Start the proxy on a random localhost port.
    /// Returns the proxy instance with the assigned port.
    pub fn start(allowed_domains: Vec<String>, workspace: String, audit_path: PathBuf) -> std::io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let port = listener.local_addr()?.port();

        let ctx = Arc::new(ProxyContext {
            domains: allowed_domains,
            workspace,
            audit_path,
        });

        let handle = thread::spawn(move || {
            for stream in listener.incoming() {
                match stream {
                    Ok(client) => {
                        let ctx = Arc::clone(&ctx);
                        thread::spawn(move || {
                            if let Err(e) = handle_client(client, &ctx) {
                                let _ = e;
                            }
                        });
                    }
                    Err(_) => break,
                }
            }
        });

        Ok(NetworkProxy {
            port,
            handle: Some(handle),
        })
    }

    /// Get the proxy port
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Get the proxy URL for env vars
    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }
}

impl Drop for NetworkProxy {
    fn drop(&mut self) {
        // Connect to self to unblock the accept loop
        let _ = TcpStream::connect(format!("127.0.0.1:{}", self.port));
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

/// Handle a single proxy client connection
fn handle_client(mut client: TcpStream, ctx: &ProxyContext) -> std::io::Result<()> {
    let mut buf = [0u8; 4096];
    let n = client.read(&mut buf)?;
    if n == 0 {
        return Ok(());
    }

    let request = String::from_utf8_lossy(&buf[..n]);
    let first_line = request.lines().next().unwrap_or("");

    if first_line.starts_with("CONNECT ") {
        handle_connect(&mut client, first_line, ctx)
    } else {
        handle_http(&mut client, &request, first_line, ctx)
    }
}

/// Handle HTTPS CONNECT tunnel
fn handle_connect(
    client: &mut TcpStream,
    first_line: &str,
    ctx: &ProxyContext,
) -> std::io::Result<()> {
    let parts: Vec<&str> = first_line.split_whitespace().collect();
    if parts.len() < 2 {
        send_response(client, 400, "Bad Request")?;
        return Ok(());
    }

    let host_port = parts[1];
    let host = host_port.split(':').next().unwrap_or("");

    if !is_domain_allowed(host, &ctx.domains) {
        log_deny(&ctx.audit_path, &ctx.workspace, host);
        send_response(client, 403, &format!("Forbidden: {} not in allowed domains", host))?;
        return Ok(());
    }

    // Connect to the target
    match TcpStream::connect(host_port) {
        Ok(mut target) => {
            // Send 200 Connection Established
            client.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")?;
            client.flush()?;

            // Bidirectional copy using cloned streams
            let mut client_read = client.try_clone()?;
            let mut client_write = client.try_clone()?;
            let mut target_clone = target.try_clone()?;

            let t1 = thread::spawn(move || {
                let _ = std::io::copy(&mut client_read, &mut target);
            });
            let t2 = thread::spawn(move || {
                let _ = std::io::copy(&mut target_clone, &mut client_write);
            });

            let _ = t1.join();
            let _ = t2.join();
        }
        Err(e) => {
            send_response(client, 502, &format!("Bad Gateway: {}", e))?;
        }
    }

    Ok(())
}

/// Handle plain HTTP request (forward if allowed)
fn handle_http(
    client: &mut TcpStream,
    full_request: &str,
    first_line: &str,
    ctx: &ProxyContext,
) -> std::io::Result<()> {
    let parts: Vec<&str> = first_line.split_whitespace().collect();
    if parts.len() < 2 {
        send_response(client, 400, "Bad Request")?;
        return Ok(());
    }

    let url = parts[1];
    let host = extract_host_from_url(url);

    if !is_domain_allowed(&host, &ctx.domains) {
        log_deny(&ctx.audit_path, &ctx.workspace, &host);
        send_response(client, 403, &format!("Forbidden: {} not in allowed domains", host))?;
        return Ok(());
    }

    // Determine target address
    let port = if url.starts_with("https://") { 443 } else { 80 };
    let target_addr = format!("{}:{}", host, port);

    match TcpStream::connect(&target_addr) {
        Ok(mut target) => {
            // Forward the request
            target.write_all(full_request.as_bytes())?;
            target.flush()?;

            // Forward the response back
            let mut response = Vec::new();
            let _ = target.read_to_end(&mut response);
            client.write_all(&response)?;
        }
        Err(e) => {
            send_response(client, 502, &format!("Bad Gateway: {}", e))?;
        }
    }

    Ok(())
}

/// Check if a domain is in the allowed list (supports wildcards)
fn is_domain_allowed(host: &str, allowed_domains: &[String]) -> bool {
    for domain in allowed_domains {
        if domain.starts_with("*.") {
            // Wildcard: *.example.com matches sub.example.com
            let suffix = &domain[1..]; // .example.com
            if host.ends_with(suffix) || host == &domain[2..] {
                return true;
            }
        } else if host == domain {
            return true;
        }
    }
    false
}

/// Extract host from a URL like http://host:port/path
fn extract_host_from_url(url: &str) -> String {
    let without_scheme = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))
        .unwrap_or(url);

    without_scheme
        .split('/')
        .next()
        .unwrap_or("")
        .split(':')
        .next()
        .unwrap_or("")
        .to_string()
}

fn send_response(client: &mut TcpStream, code: u16, message: &str) -> std::io::Result<()> {
    let response = format!(
        "HTTP/1.1 {} {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        code,
        match code {
            200 => "OK",
            400 => "Bad Request",
            403 => "Forbidden",
            502 => "Bad Gateway",
            _ => "Error",
        },
        message.len(),
        message
    );
    client.write_all(response.as_bytes())?;
    client.flush()
}

/// Log a network deny to the audit file
fn log_deny(audit_path: &std::path::Path, workspace: &str, domain: &str) {
    let entry = serde_json::json!({
        "ts": chrono::Utc::now().to_rfc3339(),
        "event": "network_denied",
        "workspace": workspace,
        "domain": domain,
    });
    if let Some(parent) = audit_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string(&entry) {
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(audit_path)
        {
            let _ = writeln!(f, "{}", json);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_domain_allowed_exact() {
        let domains = vec!["github.com".to_string(), "api.anthropic.com".to_string()];
        assert!(is_domain_allowed("github.com", &domains));
        assert!(is_domain_allowed("api.anthropic.com", &domains));
        assert!(!is_domain_allowed("evil.com", &domains));
        assert!(!is_domain_allowed("notgithub.com", &domains));
    }

    #[test]
    fn test_is_domain_allowed_wildcard() {
        let domains = vec!["*.github.com".to_string(), "example.com".to_string()];
        assert!(is_domain_allowed("api.github.com", &domains));
        assert!(is_domain_allowed("raw.github.com", &domains));
        assert!(is_domain_allowed("github.com", &domains));
        assert!(!is_domain_allowed("github.com.evil.com", &domains));
        assert!(is_domain_allowed("example.com", &domains));
    }

    #[test]
    fn test_extract_host() {
        assert_eq!(extract_host_from_url("http://github.com/path"), "github.com");
        assert_eq!(extract_host_from_url("https://api.example.com:443/v1"), "api.example.com");
        assert_eq!(extract_host_from_url("github.com/path"), "github.com");
    }
}
