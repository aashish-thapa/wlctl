// Captive-portal helpers: find the login page and open it in a browser.
//
// NetworkManager tells us *that* we're behind a portal (Connectivity::Portal)
// but not the login URL. We probe a well-known plain-HTTP endpoint; behind a
// portal the request is typically answered with a 3xx redirect to the login
// page, whose `Location` we return. When we can't determine a URL we fall back
// to a neutral HTTP page that reliably triggers the portal in the browser.

use std::process::{Command, Stdio};
use std::time::Duration;

use anyhow::{Context, Result};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;

const PROBE_HOST: &str = "connectivitycheck.gstatic.com";
const PROBE_PATH: &str = "/generate_204";
const PROBE_TIMEOUT: Duration = Duration::from_secs(3);
// Cap how much of the probe response we read. The endpoint normally returns an
// empty 204 or a small 3xx; anything larger is unexpected and not useful for
// finding a `Location` header.
const MAX_PROBE_RESPONSE_BYTES: u64 = 16 * 1024;

/// Plain-HTTP page that never redirects to HTTPS, so opening it forces a captive
/// portal to reveal its login page in the browser.
pub const FALLBACK_URL: &str = "http://neverssl.com";

/// Probes a `generate_204` endpoint and returns the captive-portal login URL if
/// the response redirects to one. Returns `None` when online (`204`) or when no
/// redirect target can be determined — callers should fall back to
/// [`FALLBACK_URL`].
pub async fn detect_portal_url() -> Option<String> {
    timeout(PROBE_TIMEOUT, probe()).await.ok().flatten()
}

async fn probe() -> Option<String> {
    let mut stream = TcpStream::connect((PROBE_HOST, 80)).await.ok()?;
    let request = format!(
        "GET {PROBE_PATH} HTTP/1.1\r\nHost: {PROBE_HOST}\r\nConnection: close\r\nUser-Agent: wlctl\r\n\r\n"
    );
    stream.write_all(request.as_bytes()).await.ok()?;

    let mut buf = Vec::new();
    stream
        .take(MAX_PROBE_RESPONSE_BYTES)
        .read_to_end(&mut buf)
        .await
        .ok()?;
    parse_redirect(&String::from_utf8_lossy(&buf))
}

/// Extracts a redirect `Location` from an HTTP response, but only for 3xx
/// statuses. Pure so it can be unit-tested without a socket.
fn parse_redirect(response: &str) -> Option<String> {
    let mut lines = response.lines();

    let status = lines.next()?;
    let code = status.split_whitespace().nth(1)?;
    if !code.starts_with('3') {
        return None;
    }

    for line in lines {
        if line.is_empty() {
            break; // end of headers
        }
        if line.to_ascii_lowercase().starts_with("location:") {
            // Slice past the ASCII "location:" prefix, preserving URL casing.
            let value = line["location:".len()..].trim();
            // The Location header is network-controlled, so only let HTTP(S)
            // URLs through. Anything else (file://, javascript:, custom app
            // schemes) could trigger an unintended local handler via xdg-open.
            let lower = value.to_ascii_lowercase();
            if lower.starts_with("http://") || lower.starts_with("https://") {
                return Some(value.to_string());
            }
        }
    }

    None
}

/// Opens a URL in the user's default browser via `xdg-open`.
pub fn open_url(url: &str) -> Result<()> {
    Command::new("xdg-open")
        .arg(url)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("failed to launch xdg-open")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redirect_extracted_from_3xx() {
        let resp = "HTTP/1.1 302 Found\r\nLocation: http://portal.example/login\r\n\r\n";
        assert_eq!(
            parse_redirect(resp).as_deref(),
            Some("http://portal.example/login")
        );
    }

    #[test]
    fn location_header_is_case_insensitive() {
        let resp = "HTTP/1.1 307 Temporary Redirect\r\nlOcAtIoN:  https://login.net/  \r\n\r\n";
        assert_eq!(parse_redirect(resp).as_deref(), Some("https://login.net/"));
    }

    #[test]
    fn no_redirect_for_204() {
        let resp = "HTTP/1.1 204 No Content\r\nLocation: http://nope\r\n\r\n";
        assert_eq!(parse_redirect(resp), None);
    }

    #[test]
    fn no_redirect_when_location_absent() {
        let resp = "HTTP/1.1 301 Moved Permanently\r\nContent-Length: 0\r\n\r\n";
        assert_eq!(parse_redirect(resp), None);
    }

    #[test]
    fn rejects_non_http_redirect_schemes() {
        // A hostile captive portal must not be able to push xdg-open into
        // launching a local handler.
        for hostile in [
            "HTTP/1.1 302 Found\r\nLocation: file:///etc/passwd\r\n\r\n",
            "HTTP/1.1 302 Found\r\nLocation: javascript:alert(1)\r\n\r\n",
            "HTTP/1.1 302 Found\r\nLocation: calculator://open\r\n\r\n",
            "HTTP/1.1 302 Found\r\nLocation: /relative/path\r\n\r\n",
        ] {
            assert_eq!(parse_redirect(hostile), None, "leaked: {hostile}");
        }
    }
}
