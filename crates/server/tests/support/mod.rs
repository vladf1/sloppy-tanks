//! What the end-to-end tests share: a server on a free localhost port with its log
//! captured, and plain HTTP requests to it. Include with `mod support;`.
#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::Value;
use sloppy_server::host::HostFactory;
use sloppy_server::server::{MultiplayerServer, ServerOptions};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// The page origin the CORS checks send.
pub const ORIGIN: &str = "http://127.0.0.1:5173";
const HTTP_WAIT: Duration = Duration::from_secs(5);

pub type Lines = Arc<Mutex<Vec<String>>>;

pub fn options(lines: &Lines) -> ServerOptions {
    let mut options = ServerOptions::new(true);
    // The rate-limit test opens sockets faster than their closes are counted.
    options.max_sockets_per_ip = 1000;
    let sink = lines.clone();
    options.log = Arc::new(move |line| sink.lock().unwrap().push(line.to_string()));
    options
}

/// A server whose rooms run `factory`'s hosts; its address and log lines.
pub async fn start_with<F: HostFactory>(factory: F) -> (MultiplayerServer, String, Lines) {
    let lines = Lines::default();
    let server = MultiplayerServer::listen(options(&lines), factory, "127.0.0.1:0")
        .await
        .unwrap();
    let base = server.local_addr().to_string();
    (server, base, lines)
}

pub struct HttpResponse {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: String,
}

/// One HTTP/1.1 request on a fresh connection, read to its end.
pub async fn http(base: &str, request_line: &str, headers: &[(&str, &str)]) -> HttpResponse {
    let mut stream = TcpStream::connect(base).await.unwrap();
    let mut request = format!("{request_line} HTTP/1.1\r\nHost: {base}\r\nConnection: close\r\n");
    for (name, value) in headers {
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    request.push_str("\r\n");
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut raw = Vec::new();
    tokio::time::timeout(HTTP_WAIT, stream.read_to_end(&mut raw))
        .await
        .unwrap()
        .unwrap();
    parse_response(&String::from_utf8(raw).unwrap())
}

pub fn parse_response(text: &str) -> HttpResponse {
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((text, ""));
    let mut lines = head.split("\r\n");
    let status = lines
        .next()
        .unwrap()
        .split(' ')
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();
    let headers = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_string()))
        .collect();
    HttpResponse {
        status,
        headers,
        body: body.to_string(),
    }
}

pub async fn get(base: &str, path: &str, headers: &[(&str, &str)]) -> HttpResponse {
    http(base, &format!("GET {path}"), headers).await
}

pub async fn rooms(base: &str, query: &str) -> Vec<Value> {
    let response = get(base, &format!("/rooms{query}"), &[("Origin", ORIGIN)]).await;
    assert_eq!(response.status, 200);
    let body: Value = serde_json::from_str(&response.body).unwrap();
    body["rooms"].as_array().unwrap().clone()
}
