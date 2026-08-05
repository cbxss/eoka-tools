use base64::Engine;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;
use url::Url;

pub struct ProxyForwarder {
    server: String,
    task: JoinHandle<()>,
}

impl ProxyForwarder {
    pub async fn start(upstream: &str, username: &str, password: &str) -> std::io::Result<Self> {
        let parsed = Url::parse(upstream).map_err(invalid_input)?;
        let host = parsed
            .host_str()
            .ok_or_else(|| invalid_input("proxy host missing"))?;
        let port = parsed
            .port()
            .ok_or_else(|| invalid_input("proxy port missing"))?;
        let upstream_addr = format!("{host}:{port}");
        let credentials =
            base64::engine::general_purpose::STANDARD.encode(format!("{username}:{password}"));
        let auth_header = format!("Basic {credentials}");
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let local_addr = listener.local_addr()?;
        let task = tokio::spawn(async move {
            while let Ok((client, _)) = listener.accept().await {
                let upstream_addr = upstream_addr.clone();
                let auth_header = auth_header.clone();
                tokio::spawn(async move {
                    let _ = handle_client(client, upstream_addr, auth_header).await;
                });
            }
        });
        Ok(Self {
            server: format!("http://{local_addr}"),
            task,
        })
    }

    pub fn server(&self) -> &str {
        &self.server
    }
}

impl Drop for ProxyForwarder {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn handle_client(
    mut client: TcpStream,
    upstream_addr: String,
    auth_header: String,
) -> std::io::Result<()> {
    let Some(request) = read_header(&mut client).await? else {
        return Ok(());
    };
    let request = String::from_utf8_lossy(&request);
    let mut upstream = TcpStream::connect(upstream_addr).await?;
    let forwarded = add_proxy_auth(&request, &auth_header);
    upstream.write_all(forwarded.as_bytes()).await?;
    let Some(response) = read_header(&mut upstream).await? else {
        return Ok(());
    };
    client.write_all(&response).await?;
    let _ = tokio::io::copy_bidirectional(&mut client, &mut upstream).await;
    Ok(())
}

async fn read_header(stream: &mut TcpStream) -> std::io::Result<Option<Vec<u8>>> {
    let mut bytes = Vec::with_capacity(4096);
    let mut buffer = [0_u8; 1];
    while bytes.len() < 64 * 1024 {
        let read = stream.read(&mut buffer).await?;
        if read == 0 {
            return if bytes.is_empty() {
                Ok(None)
            } else {
                Ok(Some(bytes))
            };
        }
        bytes.push(buffer[0]);
        if bytes.ends_with(b"\r\n\r\n") {
            return Ok(Some(bytes));
        }
    }
    Err(invalid_input("proxy request header too large"))
}

fn add_proxy_auth(request: &str, auth_header: &str) -> String {
    let mut lines = request.split("\r\n");
    let mut forwarded = String::new();
    if let Some(first) = lines.next() {
        forwarded.push_str(first);
        forwarded.push_str("\r\n");
    }
    for line in lines {
        if line.is_empty() {
            break;
        }
        let lower = line
            .split_once(':')
            .map(|(name, _)| name.trim().to_ascii_lowercase());
        if matches!(
            lower.as_deref(),
            Some("proxy-authorization" | "proxy-connection")
        ) {
            continue;
        }
        forwarded.push_str(line);
        forwarded.push_str("\r\n");
    }
    forwarded.push_str("Proxy-Authorization: ");
    forwarded.push_str(auth_header);
    forwarded.push_str("\r\nProxy-Connection: Keep-Alive\r\n\r\n");
    forwarded
}

fn invalid_input(error: impl ToString) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidInput, error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_proxy_authorization_and_preserves_request() {
        let request =
            "CONNECT example.com:443 HTTP/1.1\r\nHost: example.com:443\r\nUser-Agent: test\r\n\r\n";
        let forwarded = add_proxy_auth(request, "Basic abc123");

        assert!(forwarded.starts_with("CONNECT example.com:443 HTTP/1.1\r\n"));
        assert!(forwarded.contains("Host: example.com:443\r\n"));
        assert!(forwarded.contains("User-Agent: test\r\n"));
        assert!(forwarded.contains("Proxy-Authorization: Basic abc123\r\n"));
        assert!(forwarded.contains("Proxy-Connection: Keep-Alive\r\n"));
        assert!(forwarded.ends_with("\r\n\r\n"));
    }

    #[test]
    fn replaces_existing_proxy_authorization() {
        let request = "GET http://example.com/ HTTP/1.1\r\nProxy-Authorization: Basic old\r\nProxy-Connection: close\r\nHost: example.com\r\n\r\n";
        let forwarded = add_proxy_auth(request, "Basic new");

        assert!(!forwarded.contains("Basic old"));
        assert!(!forwarded.contains("Proxy-Connection: close"));
        assert!(forwarded.contains("Proxy-Authorization: Basic new\r\n"));
        assert!(forwarded.contains("Proxy-Connection: Keep-Alive\r\n"));
    }

    #[tokio::test]
    async fn forwards_connect_with_auth_and_tunnels_bytes() {
        let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream.local_addr().unwrap();
        let upstream_task = tokio::spawn(async move {
            let (mut stream, _) = upstream.accept().await.unwrap();
            let request = read_header(&mut stream).await.unwrap().unwrap();
            let request = String::from_utf8(request).unwrap();
            assert!(request.starts_with("CONNECT example.com:443 HTTP/1.1\r\n"));
            assert!(request.contains("Proxy-Authorization: Basic dXNlcjpwYXNz\r\n"));
            stream
                .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
                .await
                .unwrap();
            let mut payload = [0_u8; 4];
            stream.read_exact(&mut payload).await.unwrap();
            assert_eq!(&payload, b"ping");
            stream.write_all(b"pong").await.unwrap();
        });

        let forwarder = ProxyForwarder::start(&format!("http://{upstream_addr}"), "user", "pass")
            .await
            .unwrap();
        let local = Url::parse(forwarder.server()).unwrap();
        let local_addr = format!("{}:{}", local.host_str().unwrap(), local.port().unwrap());
        let mut client = TcpStream::connect(local_addr).await.unwrap();
        client
            .write_all(b"CONNECT example.com:443 HTTP/1.1\r\nHost: example.com:443\r\n\r\n")
            .await
            .unwrap();
        let response = read_header(&mut client).await.unwrap().unwrap();
        let response = String::from_utf8(response).unwrap();
        assert!(response.starts_with("HTTP/1.1 200 Connection Established\r\n"));
        client.write_all(b"ping").await.unwrap();
        let mut reply = [0_u8; 4];
        client.read_exact(&mut reply).await.unwrap();
        assert_eq!(&reply, b"pong");

        upstream_task.await.unwrap();
    }
}
