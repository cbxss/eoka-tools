use std::fmt;

use percent_encoding::percent_decode_str;
use url::Url;

#[derive(Clone, Eq, PartialEq)]
pub struct ProxyConfig {
    pub server: String,
    pub username: Option<String>,
    pub password: Option<String>,
}

impl fmt::Debug for ProxyConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProxyConfig")
            .field("server", &self.server)
            .field("authenticated", &self.username.is_some())
            .finish()
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ProxyError {
    InvalidUrl,
    UnsupportedScheme,
    MissingHost,
    MissingPort,
    InvalidPath,
    InvalidCredentials,
    InvalidLegacyFormat,
}

impl fmt::Display for ProxyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidUrl => "proxy URL is invalid",
            Self::UnsupportedScheme => "proxy URL must use socks5:// or http://",
            Self::MissingHost => "proxy URL must include a host",
            Self::MissingPort => "proxy URL must include a port",
            Self::InvalidPath => "proxy URL cannot include a path, query, or fragment",
            Self::InvalidCredentials => {
                "proxy credentials must include both a username and password"
            }
            Self::InvalidLegacyFormat => "proxy must be a URL or host:port[:username:password]",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for ProxyError {}

pub fn parse(value: &str) -> Result<ProxyConfig, ProxyError> {
    if value.contains("://") {
        parse_url(value)
    } else {
        parse_legacy(value)
    }
}

pub fn parse_server(
    server: &str,
    username: Option<String>,
    password: Option<String>,
) -> Result<ProxyConfig, ProxyError> {
    let parsed = parse_url(server)?;
    if parsed.username.is_some() || parsed.password.is_some() {
        return Err(ProxyError::InvalidCredentials);
    }
    if username.is_some() != password.is_some() {
        return Err(ProxyError::InvalidCredentials);
    }
    Ok(ProxyConfig {
        server: parsed.server,
        username,
        password,
    })
}

fn parse_url(value: &str) -> Result<ProxyConfig, ProxyError> {
    let parsed = Url::parse(value).map_err(|_| ProxyError::InvalidUrl)?;
    let scheme = parsed.scheme();
    if scheme != "socks5" && scheme != "http" {
        return Err(ProxyError::UnsupportedScheme);
    }
    if (parsed.path() != "" && parsed.path() != "/")
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err(ProxyError::InvalidPath);
    }
    let host = parsed.host_str().ok_or(ProxyError::MissingHost)?;
    let port = parsed.port().ok_or(ProxyError::MissingPort)?;
    let host = if host.contains(':') {
        format!("[{host}]")
    } else {
        host.to_owned()
    };
    let username = decode_optional(parsed.username())?;
    let password = parsed.password().map(decode).transpose()?;
    if username.is_some() != password.is_some() {
        return Err(ProxyError::InvalidCredentials);
    }
    Ok(ProxyConfig {
        server: format!("{scheme}://{host}:{port}"),
        username,
        password,
    })
}

fn parse_legacy(value: &str) -> Result<ProxyConfig, ProxyError> {
    let parts: Vec<&str> = value.splitn(4, ':').collect();
    let (server, username, password) = match parts.as_slice() {
        [host, port] if !host.is_empty() && !port.is_empty() => {
            (format!("http://{host}:{port}"), None, None)
        }
        [host, port, username, password]
            if !host.is_empty()
                && !port.is_empty()
                && !username.is_empty()
                && !password.is_empty() =>
        {
            (
                format!("http://{host}:{port}"),
                Some((*username).to_owned()),
                Some((*password).to_owned()),
            )
        }
        _ => return Err(ProxyError::InvalidLegacyFormat),
    };
    parse_server(&server, username, password)
}

fn decode_optional(value: &str) -> Result<Option<String>, ProxyError> {
    if value.is_empty() {
        Ok(None)
    } else {
        decode(value).map(Some)
    }
}

fn decode(value: &str) -> Result<String, ProxyError> {
    percent_decode_str(value)
        .decode_utf8()
        .map(|value| value.into_owned())
        .map_err(|_| ProxyError::InvalidUrl)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_authenticated_socks5_url() {
        let proxy = parse("socks5://name%40example:pass%3Aword@127.0.0.1:1080").unwrap();
        assert_eq!(proxy.server, "socks5://127.0.0.1:1080");
        assert_eq!(proxy.username.as_deref(), Some("name@example"));
        assert_eq!(proxy.password.as_deref(), Some("pass:word"));
        assert!(!format!("{proxy:?}").contains("pass:word"));
    }

    #[test]
    fn parses_legacy_proxy() {
        let proxy = parse("127.0.0.1:8080:user:password:with:colon").unwrap();
        assert_eq!(proxy.server, "http://127.0.0.1:8080");
        assert_eq!(proxy.username.as_deref(), Some("user"));
        assert_eq!(proxy.password.as_deref(), Some("password:with:colon"));
    }

    #[test]
    fn rejects_incomplete_credentials() {
        assert_eq!(
            parse("socks5://user@127.0.0.1:1080").unwrap_err(),
            ProxyError::InvalidCredentials
        );
    }

    #[test]
    fn rejects_unsupported_scheme() {
        assert_eq!(
            parse("https://127.0.0.1:1080").unwrap_err(),
            ProxyError::UnsupportedScheme
        );
    }
}
