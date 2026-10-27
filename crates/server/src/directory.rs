//! Announces this server and its rooms to the server directory (`DIRECTORY_URL`), the
//! Worker that lists every server for the room browser. The directory takes the server's
//! public address from the request's source IP, so announcements go out over IPv4 only.

use std::fmt;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use http_body_util::Full;
use hyper::{Request, StatusCode, Uri, header};
use hyper_util::rt::TokioIo;
use rustls::pki_types::ServerName;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio::time::{Instant, MissedTickBehavior};
use tokio_rustls::TlsConnector;

use crate::server::Log;

/// How often the listing is compared with the last one the directory accepted.
pub const ANNOUNCE_CHECK: Duration = Duration::from_secs(2);
/// An unchanged listing is sent again this often, so the directory knows the server is
/// still up.
pub const ANNOUNCE_INTERVAL: Duration = Duration::from_secs(15);
/// One announcement, from resolving the directory's host to its answer.
pub const ANNOUNCE_TIMEOUT: Duration = Duration::from_secs(5);

/// `DIRECTORY_URL`, the PUT target exactly as given (path and query included).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirectoryUrl {
    text: String,
    tls: bool,
    host: String,
    port: u16,
    /// The Host header: the host, with the port when the URL names one.
    authority: String,
    path_and_query: String,
}

impl DirectoryUrl {
    pub fn parse(text: &str) -> Result<Self, String> {
        let invalid = || format!("DIRECTORY_URL must be an https:// or http:// URL, not {text:?}");
        let uri: Uri = text.parse().map_err(|_| invalid())?;
        let tls = match uri.scheme_str() {
            Some("https") => true,
            Some("http") => false,
            _ => return Err(invalid()),
        };
        let host = uri
            .host()
            .filter(|host| !host.is_empty())
            .ok_or_else(invalid)?;
        let authority = match uri.port() {
            Some(port) => format!("{host}:{port}"),
            None => host.to_string(),
        };
        Ok(Self {
            text: text.to_string(),
            tls,
            host: host.to_string(),
            port: uri.port_u16().unwrap_or(if tls { 443 } else { 80 }),
            authority,
            path_and_query: uri
                .path_and_query()
                .map_or("/", |path| path.as_str())
                .to_string(),
        })
    }
}

impl fmt::Display for DirectoryUrl {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.text)
    }
}

/// Where and how often to announce; tests shorten the timings.
#[derive(Clone, Debug)]
pub struct DirectoryOptions {
    pub url: DirectoryUrl,
    pub check: Duration,
    pub interval: Duration,
    pub timeout: Duration,
}

impl DirectoryOptions {
    pub fn new(url: DirectoryUrl) -> Self {
        Self {
            url,
            check: ANNOUNCE_CHECK,
            interval: ANNOUNCE_INTERVAL,
            timeout: ANNOUNCE_TIMEOUT,
        }
    }
}

/// Every `check`, PUTs the `listing` JSON when it differs from the last one the directory
/// accepted, or when `interval` has passed since then; a failed announcement is retried
/// at the next check. Logs only when announcing starts working or starts failing.
pub async fn announce(options: DirectoryOptions, listing: impl Fn() -> String, log: Log) {
    let tls = tls_connector();
    let mut accepted: Option<(Bytes, Instant)> = None;
    let mut working: Option<bool> = None;
    let mut check = tokio::time::interval(options.check);
    check.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        check.tick().await;
        let body = Bytes::from(listing());
        if accepted
            .as_ref()
            .is_some_and(|(last, at)| *last == body && at.elapsed() < options.interval)
        {
            continue;
        }
        let result = tokio::time::timeout(options.timeout, put(&options.url, &tls, body.clone()))
            .await
            .unwrap_or_else(|_| {
                Err(format!(
                    "no answer within {} s",
                    options.timeout.as_secs_f64()
                ))
            });
        let now_working = result.is_ok();
        if working != Some(now_working) {
            log(&match &result {
                Ok(()) => format!("announcing to the server directory {}", options.url),
                Err(error) => format!(
                    "cannot announce to the server directory {}: {error}",
                    options.url
                ),
            });
        }
        working = Some(now_working);
        if now_working {
            accepted = Some((body, Instant::now()));
        }
    }
}

/// Mozilla's root certificates are compiled in: the server image has no CA store.
fn tls_connector() -> TlsConnector {
    let roots = rustls::RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
    };
    let provider = Arc::new(rustls_rustcrypto::provider());
    let config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .expect("the provider supports TLS 1.2 and 1.3")
        .with_root_certificates(roots)
        .with_no_client_auth();
    TlsConnector::from(Arc::new(config))
}

/// One PUT on a fresh connection to the directory host's first IPv4 address.
async fn put(url: &DirectoryUrl, tls: &TlsConnector, body: Bytes) -> Result<(), String> {
    let address = tokio::net::lookup_host((url.host.as_str(), url.port))
        .await
        .map_err(|error| format!("cannot resolve {}: {error}", url.host))?
        .find(SocketAddr::is_ipv4)
        .ok_or_else(|| format!("{} has no IPv4 address", url.host))?;
    let stream = TcpStream::connect(address)
        .await
        .map_err(|error| format!("cannot connect to {address}: {error}"))?;
    let request = Request::put(url.path_and_query.as_str())
        .header(header::HOST, url.authority.as_str())
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::CONNECTION, "close")
        .body(Full::new(body))
        .expect("valid request");
    let status = if url.tls {
        let name = ServerName::try_from(url.host.clone())
            .map_err(|error| format!("{}: {error}", url.host))?;
        let stream = tls
            .connect(name, stream)
            .await
            .map_err(|error| format!("TLS: {error}"))?;
        exchange(stream, request).await?
    } else {
        exchange(stream, request).await?
    };
    if status.is_success() {
        Ok(())
    } else {
        Err(format!("HTTP {status}"))
    }
}

/// Sends the request and reads the response's status. The connection runs in this future,
/// so the caller's timeout cancels all of it; it ends once the response is dropped and the
/// sender with it.
async fn exchange(
    io: impl AsyncRead + AsyncWrite + Send + Unpin + 'static,
    request: Request<Full<Bytes>>,
) -> Result<StatusCode, String> {
    let (mut sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(io))
        .await
        .map_err(|error| error.to_string())?;
    let status = async move {
        sender
            .send_request(request)
            .await
            .map(|response| response.status())
    };
    let (status, _) = tokio::join!(status, connection);
    status.map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_put_target_as_given() {
        let url = DirectoryUrl::parse(
            "https://sloppy-tanks-directory.fridman.me/servers?environment=dev",
        )
        .unwrap();
        assert!(url.tls);
        assert_eq!(
            (url.host.as_str(), url.port, url.authority.as_str()),
            (
                "sloppy-tanks-directory.fridman.me",
                443,
                "sloppy-tanks-directory.fridman.me"
            )
        );
        assert_eq!(url.path_and_query, "/servers?environment=dev");
        let local = DirectoryUrl::parse("http://127.0.0.1:8790").unwrap();
        assert!(!local.tls);
        assert_eq!(
            (
                local.port,
                local.authority.as_str(),
                local.path_and_query.as_str()
            ),
            (8790, "127.0.0.1:8790", "/")
        );
        for invalid in [
            "ftp://example.com/",
            "example.com/servers",
            "https://",
            "not a url",
        ] {
            assert!(DirectoryUrl::parse(invalid).is_err(), "{invalid}");
        }
    }
}
