//! The game's site served beside the rooms (`SITE_DIR`): routes, encodings, caching and
//! conditional requests over real localhost sockets.

mod support;

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use serde_json::Value;
use sloppy_server::config::BuildInfo;
use sloppy_server::match_room::MatchRoom;
use sloppy_server::server::MultiplayerServer;
use sloppy_server::site::Site;
use support::{Lines, options, start_with};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// `siteBuild` of `tests/fixtures/site`, which `tests/content-version.test.ts` pins for
/// `scripts/content-version.mjs` too.
const FIXTURE_SITE_BUILD: &str = "5f743dadd52fd70357e19b40";
const FILLED_INDEX: &str =
    "<!doctype html>\n<title>Fixture</title>\n<p class=\"build\">v1.1.0.628 · 27e68e8</p>\n";
const IDENTITY_JS: &[u8] = b"console.log(\"sloppy\");\n";
const ENGINE_BYTES: usize = 256 * 1024;
const TEXTURE_BYTES: usize = 200 * 1024;
const WAIT: Duration = Duration::from_secs(5);

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/site")
}

fn build() -> BuildInfo {
    BuildInfo {
        release: Some("1.1.0.628".into()),
        commit: Some("27e68e8".into()),
        dirty: false,
        built_at: None,
    }
}

/// A scratch bundle, removed when dropped.
struct Bundle(PathBuf);

impl Drop for Bundle {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn engine() -> Vec<u8> {
    (0..ENGINE_BYTES)
        .map(|index| (index * 7 % 251) as u8)
        .collect()
}

fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(bytes).unwrap();
    encoder.finish().unwrap()
}

fn gunzip(bytes: &[u8]) -> Vec<u8> {
    let mut decoded = Vec::new();
    GzDecoder::new(bytes).read_to_end(&mut decoded).unwrap();
    decoded
}

/// A long page with the build line, so its filled copy is worth compressing.
fn help_page(label: &str) -> String {
    format!(
        "<!doctype html>\n<p>{label}</p>\n{}",
        "<p>Drive, aim, fire.</p>\n".repeat(50)
    )
}

/// The fixture plus an engine with only a gzip copy, an incompressible texture and a
/// page long enough to compress.
fn bundle(name: &str) -> Bundle {
    let directory =
        std::env::temp_dir().join(format!("sloppy-site-test-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    copy_tree(&fixture(), &directory);
    let engine = engine();
    std::fs::write(directory.join("assets/engine-abc123.wasm"), &engine).unwrap();
    std::fs::write(
        directory.join("assets/engine-abc123.wasm.gz"),
        gzip(&engine),
    )
    .unwrap();
    std::fs::create_dir_all(directory.join("textures")).unwrap();
    let mut state = 0x9e37_79b9_u32;
    let texture: Vec<u8> = (0..TEXTURE_BYTES)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state as u8
        })
        .collect();
    std::fs::write(directory.join("textures/big.webp"), texture).unwrap();
    std::fs::write(
        directory.join("help.html"),
        help_page("<!--sloppy-build-label-->"),
    )
    .unwrap();
    Bundle(directory)
}

async fn start_with_site(directory: &Path) -> (MultiplayerServer, String, Arc<Site>) {
    let site = Arc::new(Site::load(directory, &build()).unwrap());
    let lines = Lines::default();
    let mut options = options(&lines);
    options.build = build();
    options.site = Some(site.clone());
    let server = MultiplayerServer::listen(options, MatchRoom::new, "127.0.0.1:0")
        .await
        .unwrap();
    let base = server.local_addr().to_string();
    (server, base, site)
}

struct Reply {
    status: u16,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

impl Reply {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).map(String::as_str)
    }
}

/// One request on a fresh connection, read to its end as bytes (bodies may be binary).
async fn request(base: &str, method: &str, path: &str, headers: &[(&str, &str)]) -> Reply {
    let mut stream = TcpStream::connect(base).await.unwrap();
    let mut text = format!("{method} {path} HTTP/1.1\r\nHost: {base}\r\nConnection: close\r\n");
    for (name, value) in headers {
        text.push_str(&format!("{name}: {value}\r\n"));
    }
    text.push_str("\r\n");
    stream.write_all(text.as_bytes()).await.unwrap();
    let mut raw = Vec::new();
    tokio::time::timeout(WAIT, stream.read_to_end(&mut raw))
        .await
        .unwrap()
        .unwrap();
    let end = raw
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .expect("a complete head");
    let head = String::from_utf8(raw[..end].to_vec()).unwrap();
    let mut lines = head.split("\r\n");
    let status = lines.next().unwrap().split(' ').nth(1).unwrap();
    Reply {
        status: status.parse().unwrap(),
        headers: lines
            .filter_map(|line| line.split_once(':'))
            .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_string()))
            .collect(),
        body: raw[end + 4..].to_vec(),
    }
}

async fn get(base: &str, path: &str, headers: &[(&str, &str)]) -> Reply {
    request(base, "GET", path, headers).await
}

#[test]
fn the_fixture_hashes_like_content_version_mjs() {
    let site = Site::load(&fixture(), &build()).unwrap();
    assert_eq!(site.build(), FIXTURE_SITE_BUILD);
    assert_eq!(
        site.file_count(),
        2,
        "the compressed copies are not files of their own"
    );
}

#[tokio::test]
async fn serves_the_game_at_the_bare_address_and_keeps_the_dashboard() {
    let bundle = bundle("routes");
    let (server, base, site) = start_with_site(&bundle.0).await;

    for path in ["/", "/index.html"] {
        let page = get(&base, path, &[]).await;
        assert_eq!(page.status, 200, "{path}");
        assert_eq!(String::from_utf8(page.body.clone()).unwrap(), FILLED_INDEX);
        assert_eq!(
            page.header("content-type"),
            Some("text/html; charset=utf-8")
        );
        assert_eq!(page.header("cache-control"), Some("no-cache"));
        assert_eq!(page.header("x-robots-tag"), Some("noindex"));
        assert_eq!(page.header("x-content-type-options"), Some("nosniff"));
        let length = FILLED_INDEX.len().to_string();
        assert_eq!(page.header("content-length"), Some(length.as_str()));
        // Too short for gzip to save anything, so there is one representation.
        assert_eq!(page.header("vary"), None);
    }

    let dashboard = get(&base, "/dashboard", &[]).await;
    assert_eq!(dashboard.status, 200);
    assert!(dashboard.header("content-security-policy").is_some());
    assert_ne!(dashboard.body, FILLED_INDEX.as_bytes());

    let health: Value = serde_json::from_slice(&get(&base, "/health", &[]).await.body).unwrap();
    assert_eq!(health["siteBuild"], site.build());
    assert!(health["contentVersion"].is_string());
    assert_eq!(get(&base, "/rooms", &[]).await.status, 200);
    server.close().await;
}

#[tokio::test]
async fn picks_the_encoding_the_client_prefers_among_the_copies_a_file_has() {
    let bundle = bundle("encodings");
    let (server, base, _) = start_with_site(&bundle.0).await;
    let script = "/assets/app-abc123.js";

    let brotli = get(&base, script, &[("Accept-Encoding", "gzip, deflate, br")]).await;
    assert_eq!(brotli.status, 200);
    assert_eq!(brotli.header("content-encoding"), Some("br"));
    assert_eq!(brotli.body, b"brotli bytes stand-in\n");
    assert!(brotli.header("etag").unwrap().ends_with("-br\""));
    assert_eq!(brotli.header("vary"), Some("Accept-Encoding"));
    assert_eq!(
        brotli.header("cache-control"),
        Some("public, max-age=31536000, immutable")
    );
    assert_eq!(
        brotli.header("content-type"),
        Some("text/javascript; charset=utf-8")
    );
    assert_eq!(brotli.header("x-robots-tag"), None);

    let gzipped = get(&base, script, &[("Accept-Encoding", "gzip")]).await;
    assert_eq!(gzipped.header("content-encoding"), Some("gzip"));
    assert_eq!(gzipped.body, b"gzip bytes stand-in\n");
    assert!(gzipped.header("etag").unwrap().ends_with("-gz\""));

    for accept in [None, Some("br;q=0, gzip;q=0")] {
        let headers: Vec<(&str, &str)> =
            accept.map(|a| ("Accept-Encoding", a)).into_iter().collect();
        let identity = get(&base, script, &headers).await;
        assert_eq!(identity.header("content-encoding"), None, "{accept:?}");
        assert_eq!(identity.body, IDENTITY_JS);
        assert_eq!(identity.header("vary"), Some("Accept-Encoding"));
        let etag = identity.header("etag").unwrap();
        assert!(etag.len() == 18 && !etag.contains('-'), "{etag}");
    }

    // The engine has only a gzip copy, which Brotli-first clients still get.
    let engine_path = "/assets/engine-abc123.wasm";
    let engine = get(&base, engine_path, &[("Accept-Encoding", "br, gzip")]).await;
    assert_eq!(engine.header("content-encoding"), Some("gzip"));
    assert_eq!(engine.header("content-type"), Some("application/wasm"));
    let length = engine.body.len().to_string();
    assert_eq!(engine.header("content-length"), Some(length.as_str()));
    assert_eq!(gunzip(&engine.body), self::engine());
    let plain = get(&base, engine_path, &[("Accept-Encoding", "br")]).await;
    assert_eq!(plain.header("content-encoding"), None);
    assert_eq!(plain.body.len(), ENGINE_BYTES);

    // A page with the build line is compressed at load, with gzip only.
    let help = get(&base, "/help.html", &[("Accept-Encoding", "br, gzip")]).await;
    assert_eq!(help.header("content-encoding"), Some("gzip"));
    assert!(help.header("etag").unwrap().ends_with("-gz\""));
    let filled = help_page("v1.1.0.628 · 27e68e8");
    assert_eq!(gunzip(&help.body), filled.as_bytes());
    let help_brotli_only = get(&base, "/help.html", &[("Accept-Encoding", "br")]).await;
    assert_eq!(help_brotli_only.body, filled.as_bytes());

    // Already compressed files have one representation and keep revalidating.
    let texture = get(
        &base,
        "/textures/big.webp",
        &[("Accept-Encoding", "br, gzip")],
    )
    .await;
    assert_eq!(texture.status, 200);
    assert_eq!(texture.header("content-encoding"), None);
    assert_eq!(texture.header("vary"), None);
    assert_eq!(texture.header("content-type"), Some("image/webp"));
    assert_eq!(texture.header("cache-control"), Some("no-cache"));
    assert_eq!(texture.body.len(), TEXTURE_BYTES);
    server.close().await;
}

#[tokio::test]
async fn answers_a_matching_if_none_match_with_304() {
    let bundle = bundle("conditional");
    let (server, base, _) = start_with_site(&bundle.0).await;
    let script = "/assets/app-abc123.js";
    let first = get(&base, script, &[("Accept-Encoding", "br")]).await;
    let etag = first.header("etag").unwrap().to_string();

    let weak = format!("W/{etag}");
    let list = format!("\"stale\", {etag}");
    for condition in [etag.as_str(), weak.as_str(), list.as_str(), "*"] {
        let headers = [("Accept-Encoding", "br"), ("If-None-Match", condition)];
        let unchanged = get(&base, script, &headers).await;
        assert_eq!(unchanged.status, 304, "{condition}");
        assert!(unchanged.body.is_empty());
        assert_eq!(unchanged.header("etag"), Some(etag.as_str()));
        assert_eq!(
            unchanged.header("cache-control"),
            Some("public, max-age=31536000, immutable")
        );
        assert_eq!(unchanged.header("vary"), Some("Accept-Encoding"));
    }

    // The Brotli tag does not match the identity representation a plain request gets.
    let other = get(&base, script, &[("If-None-Match", etag.as_str())]).await;
    assert_eq!(other.status, 200);
    assert_eq!(other.body, IDENTITY_JS);
    server.close().await;
}

#[tokio::test]
async fn head_reports_the_length_without_a_body() {
    let bundle = bundle("head");
    let (server, base, _) = start_with_site(&bundle.0).await;
    for (path, accept) in [("/assets/engine-abc123.wasm", "gzip"), ("/", "")] {
        let headers = [("Accept-Encoding", accept)];
        let full = get(&base, path, &headers).await;
        let head = request(&base, "HEAD", path, &headers).await;
        assert_eq!(head.status, 200, "{path}");
        assert!(head.body.is_empty(), "{path}");
        let length = full.body.len().to_string();
        assert_eq!(
            head.header("content-length"),
            Some(length.as_str()),
            "{path}"
        );
        assert_eq!(head.header("etag"), full.header("etag"));
        assert_eq!(
            head.header("content-encoding"),
            full.header("content-encoding")
        );
    }
    server.close().await;
}

#[tokio::test]
async fn refuses_other_methods_and_unknown_or_variant_paths() {
    let bundle = bundle("refusals");
    let (server, base, _) = start_with_site(&bundle.0).await;
    let posted = request(&base, "POST", "/index.html", &[("Content-Length", "0")]).await;
    assert_eq!(posted.status, 405);
    assert_eq!(posted.header("allow"), Some("GET, HEAD"));
    for path in [
        "/assets/app-abc123.js.br",
        "/assets/engine-abc123.wasm.gz",
        "/missing.js",
    ] {
        assert_eq!(get(&base, path, &[]).await.status, 404, "{path}");
    }
    // The query plays no part in the lookup, and a range gets the whole file.
    let queried = get(
        &base,
        "/assets/app-abc123.js?v=2",
        &[("Range", "bytes=0-3")],
    )
    .await;
    assert_eq!(queried.status, 200);
    assert_eq!(queried.body, IDENTITY_JS);
    server.close().await;
}

#[tokio::test]
async fn without_a_site_the_bare_address_stays_the_dashboard() {
    let (server, base, _) = start_with(MatchRoom::new).await;
    let bare = get(&base, "/", &[]).await;
    assert_eq!(bare.status, 200);
    assert!(bare.header("content-security-policy").is_some());
    let health: Value = serde_json::from_slice(&get(&base, "/health", &[]).await.body).unwrap();
    assert!(health.get("siteBuild").is_none());
    assert_eq!(get(&base, "/index.html", &[]).await.status, 404);
    server.close().await;
}
