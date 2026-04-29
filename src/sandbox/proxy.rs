use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;

/// A minimal HTTP CONNECT proxy that only allows connections to specified domains.
/// Runs on localhost, used inside the sandbox via http_proxy/https_proxy env vars.
/// All outbound from sandbox goes through this — domains not in the allow list are rejected.
///
/// Security note: no authentication on the proxy listener. Any local process running
/// as the same user can connect. This is intentional — the proxy only restricts access
/// (filters to allowed domains), it doesn't grant access to anything the host doesn't
/// already have. A local process could reach the same domains directly without the proxy.
/// Auth would add complexity without security benefit since the token would need to be
/// in an env var readable by any same-user process.

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

            // Read complete TLS ClientHello and validate SNI.
            // Fail closed: if we can't read a complete record or parse SNI, deny.
            let hello_buf = match read_tls_client_hello(client) {
                Ok(buf) => buf,
                Err(_) => {
                    // Can't read TLS record — not a TLS connection or client error
                    // Fail closed: deny
                    log_deny(&ctx.audit_path, &ctx.workspace,
                        &format!("{}(no-tls-record)", host));
                    return Ok(());
                }
            };

            // Extract and validate SNI — fail closed on None
            match extract_tls_sni(&hello_buf) {
                Some(sni) => {
                    if sni != host && !host_matches_sni(host, &sni) {
                        log_deny(&ctx.audit_path, &ctx.workspace,
                            &format!("{}(sni:{})", host, sni));
                        return Ok(());
                    }
                }
                None => {
                    // Can't extract SNI — fail closed
                    log_deny(&ctx.audit_path, &ctx.workspace,
                        &format!("{}(sni-parse-failed)", host));
                    return Ok(());
                }
            }

            // Forward the validated ClientHello to target
            target.write_all(&hello_buf)?;
            target.flush()?;

            // Bidirectional copy for the rest
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
    let url_host = extract_host_from_url(url);

    // Also extract and validate Host header
    let header_host = full_request.lines()
        .find(|l| l.to_lowercase().starts_with("host:"))
        .map(|l| l[5..].trim().split(':').next().unwrap_or("").to_string())
        .unwrap_or_default();

    // Use URL host primarily, but verify Host header matches if present
    let host = if !url_host.is_empty() {
        &url_host
    } else if !header_host.is_empty() {
        &header_host
    } else {
        send_response(client, 400, "No host specified")?;
        return Ok(());
    };

    // If both exist, they must match (prevent Host header smuggling)
    if !url_host.is_empty() && !header_host.is_empty()
        && url_host != header_host
    {
        log_deny(&ctx.audit_path, &ctx.workspace,
            &format!("{}(host-mismatch:{})", url_host, header_host));
        send_response(client, 400, "Host header mismatch")?;
        return Ok(());
    }

    if !is_domain_allowed(host, &ctx.domains) {
        log_deny(&ctx.audit_path, &ctx.workspace, host);
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

/// Read a complete TLS ClientHello record from the client.
/// Uses the TLS record header to determine exact length, reads until complete.
/// Caps at 16KB (TLS max record size). Times out after 5 seconds.
/// Returns the full record bytes or an error.
fn read_tls_client_hello(client: &mut TcpStream) -> std::io::Result<Vec<u8>> {
    use std::time::Duration;

    // Set read timeout to prevent blocking forever
    client.set_read_timeout(Some(Duration::from_secs(5)))?;

    // Read TLS record header (5 bytes): type(1) + version(2) + length(2)
    let mut header = [0u8; 5];
    client.read_exact(&mut header)?;

    // Validate: must be a TLS handshake record
    if header[0] != 0x16 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "not a TLS handshake record",
        ));
    }

    // Extract record payload length
    let record_len = u16::from_be_bytes([header[3], header[4]]) as usize;

    // Sanity check: TLS records max 16KB, ClientHello typically < 10KB
    if record_len > 16384 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "TLS record too large",
        ));
    }

    // Read the complete record payload
    let mut payload = vec![0u8; record_len];
    client.read_exact(&mut payload)?;

    // Clear read timeout
    client.set_read_timeout(None)?;

    // Combine header + payload
    let mut full = Vec::with_capacity(5 + record_len);
    full.extend_from_slice(&header);
    full.extend_from_slice(&payload);

    // Validate: must be a ClientHello (handshake type 0x01)
    if payload.is_empty() || payload[0] != 0x01 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "not a ClientHello",
        ));
    }

    Ok(full)
}

/// Extract SNI (Server Name Indication) from a TLS ClientHello message.
/// Returns None if the data isn't a valid ClientHello or has no SNI extension.
///
/// TLS record format:
///   byte 0: content type (0x16 = handshake)
///   bytes 1-2: TLS version
///   bytes 3-4: record length
///   byte 5: handshake type (0x01 = ClientHello)
///   ... extensions contain SNI at type 0x0000
fn extract_tls_sni(data: &[u8]) -> Option<String> {
    // Minimum TLS record: 5 byte header + 1 byte handshake type
    if data.len() < 6 { return None; }

    // Check TLS handshake record
    if data[0] != 0x16 { return None; } // Not a handshake
    if data[5] != 0x01 { return None; } // Not ClientHello

    // Skip: record header (5) + handshake header (4) + client version (2) + random (32)
    let mut pos = 5 + 4 + 2 + 32;
    if pos >= data.len() { return None; }

    // Session ID (length-prefixed)
    let session_id_len = data[pos] as usize;
    pos += 1 + session_id_len;
    if pos + 2 > data.len() { return None; }

    // Cipher suites (2-byte length prefix)
    let cipher_suites_len = u16::from_be_bytes([data[pos], data[pos + 1]]) as usize;
    pos += 2 + cipher_suites_len;
    if pos + 1 > data.len() { return None; }

    // Compression methods (1-byte length prefix)
    let compression_len = data[pos] as usize;
    pos += 1 + compression_len;
    if pos + 2 > data.len() { return None; }

    // Extensions (2-byte length prefix)
    let extensions_len = u16::from_be_bytes([data[pos], data[pos + 1]]) as usize;
    pos += 2;
    let extensions_end = pos + extensions_len;
    if extensions_end > data.len() { return None; }

    // Walk extensions looking for SNI (type 0x0000)
    while pos + 4 <= extensions_end {
        let ext_type = u16::from_be_bytes([data[pos], data[pos + 1]]);
        let ext_len = u16::from_be_bytes([data[pos + 2], data[pos + 3]]) as usize;
        pos += 4;

        if ext_type == 0x0000 && ext_len > 0 {
            // SNI extension: list length (2) + type (1) + name length (2) + name
            if pos + 5 > data.len() { return None; }
            let _list_len = u16::from_be_bytes([data[pos], data[pos + 1]]);
            let _name_type = data[pos + 2]; // 0 = hostname
            let name_len = u16::from_be_bytes([data[pos + 3], data[pos + 4]]) as usize;
            pos += 5;
            if pos + name_len > data.len() { return None; }
            return String::from_utf8(data[pos..pos + name_len].to_vec()).ok();
        }

        pos += ext_len;
    }

    None
}

/// Check if the CONNECT host matches the TLS SNI.
/// Allows exact match and same base domain (e.g., host=github.com, sni=github.com)
fn host_matches_sni(connect_host: &str, sni: &str) -> bool {
    // Exact match
    if connect_host == sni { return true; }
    // SNI is a subdomain of connect host (e.g., CONNECT github.com, SNI www.github.com)
    if sni.ends_with(&format!(".{}", connect_host)) { return true; }
    // Connect host is a subdomain of SNI (e.g., CONNECT www.github.com, SNI github.com)
    if connect_host.ends_with(&format!(".{}", sni)) { return true; }
    false
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
    if let Ok(json) = serde_json::to_string(&entry) {
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(audit_path)
        {
            let _ = writeln!(f, "{}", json);
            // Set 0600 on audit log
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(audit_path, std::fs::Permissions::from_mode(0o600));
            }
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

    #[test]
    fn test_sni_extraction() {
        // Minimal TLS 1.2 ClientHello with SNI=example.com
        // This is a hand-crafted minimal ClientHello
        let mut hello = Vec::new();
        // Record header: handshake (0x16), TLS 1.0 (0x0301), length placeholder
        hello.extend_from_slice(&[0x16, 0x03, 0x01, 0x00, 0x00]);
        // Handshake header: ClientHello (0x01), length placeholder
        hello.extend_from_slice(&[0x01, 0x00, 0x00, 0x00]);
        // Client version: TLS 1.2
        hello.extend_from_slice(&[0x03, 0x03]);
        // Random: 32 bytes
        hello.extend_from_slice(&[0u8; 32]);
        // Session ID: length 0
        hello.push(0x00);
        // Cipher suites: length 2, one suite
        hello.extend_from_slice(&[0x00, 0x02, 0x00, 0x2f]);
        // Compression: length 1, null
        hello.extend_from_slice(&[0x01, 0x00]);

        // Extensions
        let sni_name = b"example.com";
        let sni_ext_len = 5 + sni_name.len(); // list_len(2) + type(1) + name_len(2) + name
        let ext_total = 4 + sni_ext_len; // ext_type(2) + ext_len(2) + payload

        hello.extend_from_slice(&[(ext_total >> 8) as u8, (ext_total & 0xff) as u8]); // extensions length
        // SNI extension: type 0x0000
        hello.extend_from_slice(&[0x00, 0x00]);
        hello.extend_from_slice(&[(sni_ext_len >> 8) as u8, (sni_ext_len & 0xff) as u8]);
        // SNI list: list_len, type=hostname(0), name_len, name
        let list_len = 3 + sni_name.len();
        hello.extend_from_slice(&[(list_len >> 8) as u8, (list_len & 0xff) as u8]);
        hello.push(0x00); // hostname type
        hello.extend_from_slice(&[(sni_name.len() >> 8) as u8, (sni_name.len() & 0xff) as u8]);
        hello.extend_from_slice(sni_name);

        // Fix lengths
        let record_payload = hello.len() - 5;
        hello[3] = (record_payload >> 8) as u8;
        hello[4] = (record_payload & 0xff) as u8;
        let handshake_payload = hello.len() - 9;
        hello[6] = 0;
        hello[7] = (handshake_payload >> 8) as u8;
        hello[8] = (handshake_payload & 0xff) as u8;

        assert_eq!(extract_tls_sni(&hello), Some("example.com".to_string()));
    }

    #[test]
    fn test_sni_not_tls() {
        assert_eq!(extract_tls_sni(b"GET / HTTP/1.1\r\n"), None);
        assert_eq!(extract_tls_sni(&[]), None);
        assert_eq!(extract_tls_sni(&[0x16, 0x03, 0x01]), None);
    }

    #[test]
    fn test_host_matches_sni() {
        assert!(host_matches_sni("github.com", "github.com"));
        assert!(host_matches_sni("github.com", "www.github.com"));
        assert!(host_matches_sni("www.github.com", "github.com"));
        assert!(!host_matches_sni("github.com", "evil.com"));
        assert!(!host_matches_sni("github.com", "notgithub.com"));
    }
}
