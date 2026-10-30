//! The server directory, read once: which game server hosts a room, or which one a new
//! room opens on. One GET over HTTPS with the server's TLS setup (Mozilla's roots and the
//! pure-Rust crypto provider).

use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use http_body_util::{BodyExt, Empty};
use hyper::{Request, Uri, header};
use hyper_util::rt::TokioIo;
use rustls::pki_types::ServerName;
use serde_json::Value;
use sloppy_core::net::room_list::ServerList;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;

const LIST_TIMEOUT: Duration = Duration::from_secs(8);

/// TLS for wss: game servers and the directory: Mozilla's roots, compiled in.
pub fn tls_config() -> Arc<rustls::ClientConfig> {
    let roots = rustls::RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
    };
    let provider = Arc::new(rustls_rustcrypto::provider());
    let config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .expect("the provider supports TLS 1.2 and 1.3")
        .with_root_certificates(roots)
        .with_no_client_auth();
    Arc::new(config)
}

/// The servers and rooms the directory at `base` lists for `environment`.
pub async fn list(
    base: &str,
    environment: &str,
    tls: Arc<rustls::ClientConfig>,
) -> Result<ServerList, String> {
    let address = format!(
        "{}/servers?environment={environment}",
        base.trim_end_matches('/')
    );
    let uri: Uri = address
        .parse()
        .map_err(|_| format!("Invalid directory address {base:?}"))?;
    let secure = match uri.scheme_str() {
        Some("https") => true,
        Some("http") => false,
        _ => {
            return Err(format!(
                "The directory needs an http(s) address, not {base:?}"
            ));
        }
    };
    let host = uri
        .host()
        .ok_or("The directory address has no host")?
        .to_string();
    let port = uri.port_u16().unwrap_or(if secure { 443 } else { 80 });
    let local = matches!(host.as_str(), "localhost" | "127.0.0.1" | "[::1]");
    let authority = uri
        .authority()
        .map_or(host.clone(), |authority| authority.to_string());
    let path = uri
        .path_and_query()
        .map_or("/", |path| path.as_str())
        .to_string();
    let fetch = async {
        let stream = TcpStream::connect((host.as_str(), port))
            .await
            .map_err(|error| format!("Cannot reach {host}:{port}: {error}"))?;
        let request = Request::get(path)
            .header(header::HOST, authority)
            .header(header::CONNECTION, "close")
            .body(Empty::<Bytes>::new())
            .expect("valid request");
        if secure {
            let name =
                ServerName::try_from(host.clone()).map_err(|error| format!("{host}: {error}"))?;
            let stream = TlsConnector::from(tls)
                .connect(name, stream)
                .await
                .map_err(|error| format!("TLS: {error}"))?;
            get(stream, request).await
        } else {
            get(stream, request).await
        }
    };
    let body = tokio::time::timeout(LIST_TIMEOUT, fetch)
        .await
        .map_err(|_| format!("{address} did not answer"))??;
    let value: Value =
        serde_json::from_slice(&body).map_err(|error| format!("{address}: {error}"))?;
    ServerList::read(&value, local)
}

async fn get(
    io: impl AsyncRead + AsyncWrite + Send + Unpin + 'static,
    request: Request<Empty<Bytes>>,
) -> Result<Bytes, String> {
    let (mut sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(io))
        .await
        .map_err(|error| error.to_string())?;
    tokio::spawn(connection);
    let response = sender
        .send_request(request)
        .await
        .map_err(|error| error.to_string())?;
    let status = response.status();
    let body = response
        .into_body()
        .collect()
        .await
        .map_err(|error| error.to_string())?
        .to_bytes();
    if status.is_success() {
        Ok(body)
    } else {
        Err(format!("HTTP {status}: {}", String::from_utf8_lossy(&body)))
    }
}
