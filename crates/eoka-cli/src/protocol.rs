use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Debug, Serialize, Deserialize)]
pub struct Request {
    pub cmd: String,
    #[serde(default)]
    pub args: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Response {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Response {
    pub fn ok(data: impl Into<serde_json::Value>) -> Self {
        Self {
            ok: true,
            data: Some(data.into()),
            error: None,
        }
    }

    pub fn ok_text(msg: impl Into<String>) -> Self {
        Self::ok(serde_json::Value::String(msg.into()))
    }

    pub fn err(msg: impl Into<String>) -> Self {
        Self {
            ok: false,
            data: None,
            error: Some(msg.into()),
        }
    }
}

/// Write a length-prefixed JSON message.
pub async fn write_msg<W: AsyncWriteExt + Unpin>(
    writer: &mut W,
    msg: &impl Serialize,
) -> std::io::Result<()> {
    let payload = serde_json::to_vec(msg).map_err(|e| std::io::Error::other(e.to_string()))?;
    let len = (payload.len() as u32).to_be_bytes();
    writer.write_all(&len).await?;
    writer.write_all(&payload).await?;
    writer.flush().await?;
    Ok(())
}

/// Read a length-prefixed JSON message.
pub async fn read_msg<R: AsyncReadExt + Unpin, T: for<'de> Deserialize<'de>>(
    reader: &mut R,
) -> std::io::Result<T> {
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;

    if len > 64 * 1024 * 1024 {
        return Err(std::io::Error::other("message too large (>64MB)"));
    }

    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf).await?;
    serde_json::from_slice(&buf).map_err(|e| std::io::Error::other(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::{UnixListener, UnixStream};

    fn temp_socket_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "eoka-protocol-test-{}-{}.sock",
            std::process::id(),
            name
        ))
    }

    #[tokio::test]
    async fn round_trip_over_unix_socket() {
        let sock_path = temp_socket_path("roundtrip");
        let _ = std::fs::remove_file(&sock_path);
        let listener = UnixListener::bind(&sock_path).unwrap();

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (mut reader, mut writer) = stream.into_split();
            let req: Request = read_msg(&mut reader).await.unwrap();
            assert_eq!(req.cmd, "ping");
            assert_eq!(req.args, serde_json::json!({ "n": 1 }));
            write_msg(&mut writer, &Response::ok_text("pong"))
                .await
                .unwrap();
        });

        let stream = UnixStream::connect(&sock_path).await.unwrap();
        let (mut reader, mut writer) = stream.into_split();
        write_msg(
            &mut writer,
            &Request {
                cmd: "ping".into(),
                args: serde_json::json!({ "n": 1 }),
            },
        )
        .await
        .unwrap();
        let response: Response = read_msg(&mut reader).await.unwrap();

        server.await.unwrap();
        let _ = std::fs::remove_file(&sock_path);

        assert!(response.ok);
        assert_eq!(
            response.data,
            Some(serde_json::Value::String("pong".into()))
        );
        assert_eq!(response.error, None);
    }

    #[tokio::test]
    async fn read_msg_rejects_oversized_length_prefix() {
        let sock_path = temp_socket_path("oversized");
        let _ = std::fs::remove_file(&sock_path);
        let listener = UnixListener::bind(&sock_path).unwrap();

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (mut reader, _writer) = stream.into_split();
            let result: std::io::Result<Request> = read_msg(&mut reader).await;
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("too large"));
        });

        let mut stream = UnixStream::connect(&sock_path).await.unwrap();
        // Claim a 65MB payload without sending one; read_msg must reject the
        // length prefix before attempting to read the (nonexistent) body.
        let oversized_len: u32 = 65 * 1024 * 1024;
        stream
            .write_all(&oversized_len.to_be_bytes())
            .await
            .unwrap();

        server.await.unwrap();
        let _ = std::fs::remove_file(&sock_path);
    }
}
