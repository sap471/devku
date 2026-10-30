use anyhow::{Context, Result};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub const ADMIN_PORT: u16 = 2019;

/// Caddy admin API hanya bicara HTTP polos di loopback, jadi tidak perlu
/// klien HTTP penuh — cukup satu request dan baca status line.
pub async fn post_caddyfile(caddyfile: &str) -> Result<u16> {
    let body = caddyfile.as_bytes();
    let request = format!(
        "POST /load HTTP/1.1\r\n\
         Host: localhost:{ADMIN_PORT}\r\n\
         Content-Type: text/caddyfile\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\
         \r\n",
        body.len()
    );

    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", ADMIN_PORT))
        .await
        .with_context(|| format!("admin API Caddy tidak menjawab di :{ADMIN_PORT}"))?;

    stream.write_all(request.as_bytes()).await?;
    stream.write_all(body).await?;
    stream.flush().await?;

    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await?;
    parse_status(&buf)
}

pub async fn ping() -> bool {
    tokio::net::TcpStream::connect(("127.0.0.1", ADMIN_PORT))
        .await
        .is_ok()
}

fn parse_status(raw: &[u8]) -> Result<u16> {
    let text = String::from_utf8_lossy(raw);
    let line = text.lines().next().unwrap_or_default();
    let code = line
        .split_whitespace()
        .nth(1)
        .and_then(|c| c.parse::<u16>().ok())
        .with_context(|| format!("respons admin API tidak dikenali: {line}"))?;
    Ok(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_status_line() {
        assert_eq!(parse_status(b"HTTP/1.1 200 OK\r\n\r\n").unwrap(), 200);
        assert_eq!(parse_status(b"HTTP/1.1 400 Bad Request\r\n").unwrap(), 400);
        assert!(parse_status(b"garbage").is_err());
    }
}
