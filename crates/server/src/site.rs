//! The game's page, scripts, engines and public assets, served from memory beside the
//! rooms.
//!
//! The server image carries the site bundle `scripts/build-server-site.mjs` builds, and
//! `SITE_DIR` names it. Every file is read once at startup together with the Brotli and
//! gzip copies the build stored beside it (`X.br`, `X.gz`), so a request only looks up a
//! table and no request path ever reaches the filesystem. Vite names the files under
//! `assets/` by their content, so browsers keep them for good; every other file keeps
//! its name from one release to the next, so browsers revalidate it by its ETag.

use std::collections::{BTreeMap, HashMap};
use std::io::Write;
use std::path::Path;

use bytes::Bytes;
use flate2::Compression;
use flate2::write::GzEncoder;
use http_body_util::Full;
use hyper::header::{self, HeaderMap, HeaderValue};
use hyper::{Method, Response, StatusCode};

use crate::config::BuildInfo;

/// Where the bundle's HTML shows Battle Setup's build line. The server writes in the
/// release and commit `/health` reports, so the bundle's bytes do not depend on the
/// commit that built them, and a commit that leaves the page alone leaves the image
/// (and the running server) alone too.
pub const BUILD_LABEL_PLACEHOLDER: &str = "<!--sloppy-build-label-->";
/// The page the bare address serves.
const INDEX: &str = "index.html";
/// Vite's output directory, whose file names carry a hash of their content.
const FINGERPRINTED_DIRECTORY: &str = "assets/";
const IMMUTABLE: &str = "public, max-age=31536000, immutable";
/// For names that stay the same across releases: the browser asks every time, and a
/// matching ETag costs only a 304.
const REVALIDATE: &str = "no-cache";
const ALLOWED_METHODS: &str = "GET, HEAD";
const ETAG_HEX_CHARS: usize = 16;
/// As long as the content version and server build beside it in `/health`.
const SITE_BUILD_HEX_CHARS: usize = 24;

/// A precompressed variant's content coding, in order of preference: Brotli wins ties.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Encoding {
    Brotli,
    Gzip,
}

const ENCODINGS: [Encoding; 2] = [Encoding::Brotli, Encoding::Gzip];

impl Encoding {
    /// The `Accept-Encoding` and `Content-Encoding` token.
    fn token(self) -> &'static str {
        match self {
            Self::Brotli => "br",
            Self::Gzip => "gzip",
        }
    }

    /// The precompressed sibling's file extension, also the variant's ETag suffix.
    fn extension(self) -> &'static str {
        match self {
            Self::Brotli => "br",
            Self::Gzip => "gz",
        }
    }
}

/// One encoding of a file: the bytes sent and their strong ETag.
struct Representation {
    bytes: Bytes,
    etag: HeaderValue,
}

struct SiteFile {
    content_type: &'static str,
    cache_control: &'static str,
    html: bool,
    identity: Representation,
    brotli: Option<Representation>,
    gzip: Option<Representation>,
}

impl SiteFile {
    fn variant(&self, encoding: Encoding) -> Option<&Representation> {
        match encoding {
            Encoding::Brotli => self.brotli.as_ref(),
            Encoding::Gzip => self.gzip.as_ref(),
        }
    }

    fn encodings(&self) -> Vec<Encoding> {
        ENCODINGS
            .into_iter()
            .filter(|encoding| self.variant(*encoding).is_some())
            .collect()
    }

    fn stored_bytes(&self) -> usize {
        [
            Some(&self.identity),
            self.brotli.as_ref(),
            self.gzip.as_ref(),
        ]
        .into_iter()
        .flatten()
        .map(|representation| representation.bytes.len())
        .sum()
    }
}

/// The loaded bundle: every file by its path relative to `SITE_DIR`.
pub struct Site {
    files: HashMap<String, SiteFile>,
    build: String,
}

impl Site {
    /// Reads the bundle in `directory`, filling the build line from `build`. A bundle
    /// without `index.html` is refused, like an unreadable one: a typo in `SITE_DIR`
    /// must stop startup rather than serve nothing.
    pub fn load(directory: &Path, build: &BuildInfo) -> Result<Self, String> {
        let mut shipped = read_tree(directory)?;
        if !shipped.contains_key(INDEX) {
            return Err(format!("{} has no {INDEX}", directory.display()));
        }
        let variants: Vec<String> = shipped
            .keys()
            .filter(|path| is_variant(path, &shipped))
            .cloned()
            .collect();
        let mut variant_bytes: HashMap<String, Vec<u8>> = variants
            .iter()
            .filter_map(|path| shipped.remove_entry(path))
            .collect();
        let site_build = site_build(&shipped);
        let label = escape_html(&build_label(build));
        let files = shipped
            .into_iter()
            .map(|(path, bytes)| {
                let mut sibling = |encoding: Encoding| {
                    variant_bytes.remove(&format!("{path}.{}", encoding.extension()))
                };
                let (brotli, gzip) = (sibling(Encoding::Brotli), sibling(Encoding::Gzip));
                let file = site_file(&path, bytes, &label, brotli, gzip);
                (path, file)
            })
            .collect();
        Ok(Self {
            files,
            build: site_build,
        })
    }

    /// A hash of the shipped files, which `/health` reports and the image labels: the
    /// same as `siteBuild` in `scripts/content-version.mjs`.
    pub fn build(&self) -> &str {
        &self.build
    }

    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    /// The memory the files take, all encodings included.
    pub fn stored_bytes(&self) -> usize {
        self.files.values().map(SiteFile::stored_bytes).sum()
    }

    /// The response for `path` (the request's path, query excluded), or `None` when the
    /// site has no such file. The bare address serves `index.html`. A `Range` header is
    /// ignored: the whole file is a valid answer, and the game never asks for part of one.
    pub fn respond(
        &self,
        method: &Method,
        path: &str,
        headers: &HeaderMap,
    ) -> Option<Response<Full<Bytes>>> {
        let name = match path {
            "/" => INDEX,
            _ => path.strip_prefix('/')?,
        };
        let file = self.files.get(name)?;
        if method != Method::GET && method != Method::HEAD {
            let response = Response::builder()
                .status(StatusCode::METHOD_NOT_ALLOWED)
                .header(header::ALLOW, ALLOWED_METHODS)
                .body(Full::default());
            return Some(response.expect("valid response"));
        }
        let encodings = file.encodings();
        let encoding =
            preferred_encoding(&header_list(headers, header::ACCEPT_ENCODING), &encodings);
        let representation = encoding
            .and_then(|encoding| file.variant(encoding))
            .unwrap_or(&file.identity);
        let mut response = Response::builder()
            .header(header::ETAG, representation.etag.clone())
            .header(header::CACHE_CONTROL, file.cache_control);
        if !encodings.is_empty() {
            response = response.header(header::VARY, "Accept-Encoding");
        }
        if none_match(headers, &representation.etag) {
            let response = response.status(StatusCode::NOT_MODIFIED);
            return Some(response.body(Full::default()).expect("valid response"));
        }
        response = response
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, file.content_type)
            .header(header::CONTENT_LENGTH, representation.bytes.len())
            .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff");
        if let Some(encoding) = encoding {
            response = response.header(header::CONTENT_ENCODING, encoding.token());
        }
        if file.html {
            // The canonical site is the static host's; this copy only keeps the game
            // beside its server.
            response = response.header("X-Robots-Tag", "noindex");
        }
        // A HEAD answer keeps the real Content-Length, which hyper leaves alone when the
        // body is empty.
        let body = if method == Method::HEAD {
            Bytes::new()
        } else {
            representation.bytes.clone()
        };
        Some(response.body(Full::new(body)).expect("valid response"))
    }
}

/// Every regular file under `root` by its relative path with forward slashes, sorted
/// bytewise (as `siteBuild` hashes them).
fn read_tree(root: &Path) -> Result<BTreeMap<String, Vec<u8>>, String> {
    let unreadable = |path: &Path, error: std::io::Error| {
        format!("Cannot read the site at {}: {error}", path.display())
    };
    let mut files = BTreeMap::new();
    let mut pending = vec![(root.to_path_buf(), String::new())];
    while let Some((directory, prefix)) = pending.pop() {
        let entries =
            std::fs::read_dir(&directory).map_err(|error| unreadable(&directory, error))?;
        for entry in entries {
            let entry = entry.map_err(|error| unreadable(&directory, error))?;
            let path = entry.path();
            let Ok(name) = entry.file_name().into_string() else {
                return Err(format!("{} is not a UTF-8 file name", path.display()));
            };
            let file_type = entry
                .file_type()
                .map_err(|error| unreadable(&path, error))?;
            if file_type.is_dir() {
                pending.push((path, format!("{prefix}{name}/")));
            } else {
                let bytes = std::fs::read(&path).map_err(|error| unreadable(&path, error))?;
                files.insert(format!("{prefix}{name}"), bytes);
            }
        }
    }
    Ok(files)
}

/// Whether `path` is the precompressed copy of another shipped file.
fn is_variant(path: &str, shipped: &BTreeMap<String, Vec<u8>>) -> bool {
    ENCODINGS.into_iter().any(|encoding| {
        path.strip_suffix(encoding.extension())
            .and_then(|stem| stem.strip_suffix('.'))
            .is_some_and(|original| shipped.contains_key(original))
    })
}

fn sha1_hex(bytes: &[u8]) -> String {
    sha1_smol::Sha1::from(bytes).digest().to_string()
}

/// SHA-1 over each shipped file's path, a NUL and its bytes as built (before the build
/// line is filled in), in path order; its first 24 hex digits.
fn site_build(files: &BTreeMap<String, Vec<u8>>) -> String {
    let mut hash = sha1_smol::Sha1::new();
    for (path, bytes) in files {
        hash.update(path.as_bytes());
        hash.update(b"\0");
        hash.update(bytes);
    }
    let mut hex = hash.digest().to_string();
    hex.truncate(SITE_BUILD_HEX_CHARS);
    hex
}

fn site_file(
    path: &str,
    bytes: Vec<u8>,
    label: &str,
    brotli: Option<Vec<u8>>,
    gzip: Option<Vec<u8>>,
) -> SiteFile {
    let html = extension(path) == "html";
    let filled = if html {
        fill_build_label(bytes, label)
    } else {
        Err(bytes)
    };
    let (bytes, brotli, gzip) = match filled {
        // The build's compressed copies hold the placeholder, so the filled page is
        // compressed here: one small file, and gzip needs no extra crate.
        Ok(page) => {
            let gzip = gzip_if_smaller(&page);
            (page, None, gzip)
        }
        Err(bytes) => (bytes, brotli, gzip),
    };
    let digest = sha1_hex(&bytes);
    let tag = &digest[..ETAG_HEX_CHARS];
    let representation = |bytes: Vec<u8>, suffix: &str| Representation {
        bytes: Bytes::from(bytes),
        etag: HeaderValue::from_str(&format!("\"{tag}{suffix}\"")).expect("hex ETag"),
    };
    let variant = |bytes: Option<Vec<u8>>, encoding: Encoding| {
        bytes.map(|bytes| representation(bytes, &format!("-{}", encoding.extension())))
    };
    SiteFile {
        content_type: content_type(path),
        cache_control: if path.starts_with(FINGERPRINTED_DIRECTORY) {
            IMMUTABLE
        } else {
            REVALIDATE
        },
        html,
        brotli: variant(brotli, Encoding::Brotli),
        gzip: variant(gzip, Encoding::Gzip),
        identity: representation(bytes, ""),
    }
}

/// The page with the build line written in, or its bytes unchanged (`Err`) when it has
/// no placeholder.
fn fill_build_label(bytes: Vec<u8>, label: &str) -> Result<Vec<u8>, Vec<u8>> {
    match String::from_utf8(bytes) {
        Ok(text) if text.contains(BUILD_LABEL_PLACEHOLDER) => {
            Ok(text.replace(BUILD_LABEL_PLACEHOLDER, label).into_bytes())
        }
        Ok(text) => Err(text.into_bytes()),
        Err(error) => Err(error.into_bytes()),
    }
}

fn gzip_if_smaller(bytes: &[u8]) -> Option<Vec<u8>> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::best());
    encoder.write_all(bytes).expect("in-memory gzip");
    let compressed = encoder.finish().expect("in-memory gzip");
    (compressed.len() < bytes.len()).then_some(compressed)
}

/// Battle Setup's build line, as `buildLabel` in `scripts/page-health.ts` writes it:
/// `v1.1.0.628 · 27e68e8`, `+` marking local changes.
fn build_label(build: &BuildInfo) -> String {
    let dirty = if build.dirty { "+" } else { "" };
    match (&build.release, &build.commit) {
        (Some(release), Some(commit)) => format!("v{release} · {commit}{dirty}"),
        (Some(release), None) => format!("v{release}"),
        (None, Some(commit)) => format!("{commit}{dirty}"),
        (None, None) => "local build".to_string(),
    }
}

fn escape_html(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(character),
        }
    }
    escaped
}

fn extension(path: &str) -> String {
    Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

fn content_type(path: &str) -> &'static str {
    match extension(path).as_str() {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "txt" => "text/plain; charset=utf-8",
        "md" => "text/markdown; charset=utf-8",
        // `WebAssembly.instantiateStreaming` refuses any other type.
        "wasm" => "application/wasm",
        "json" => "application/json",
        "webmanifest" => "application/manifest+json",
        "svg" => "image/svg+xml",
        "webp" => "image/webp",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "ico" => "image/x-icon",
        "mp3" => "audio/mpeg",
        "ogg" => "audio/ogg",
        "wav" => "audio/wav",
        "woff2" => "font/woff2",
        _ => "application/octet-stream",
    }
}

/// Every value of a list header, joined as one list.
fn header_list(headers: &HeaderMap, name: header::HeaderName) -> String {
    headers
        .get_all(name)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .collect::<Vec<_>>()
        .join(",")
}

/// The client's preferred coding among `available`, as `preferredEncoding` in
/// `deploy/dev-site/worker.mjs` picks it: each coding's quality (1 without `q`, 0 when
/// invalid or out of range), `*` for codings not named, Brotli on a tie, and `None`
/// (the file as is) when nothing available is acceptable.
pub fn preferred_encoding(accept_encoding: &str, available: &[Encoding]) -> Option<Encoding> {
    let mut qualities = HashMap::new();
    for entry in accept_encoding.to_ascii_lowercase().split(',') {
        let mut parts = entry.split(';');
        let coding = parts.next().unwrap_or("").trim().to_string();
        let mut quality = 1.0;
        for parameter in parts {
            if let Some(value) = parameter.trim().strip_prefix("q=") {
                quality = value.trim().parse().unwrap_or(f64::NAN);
                break;
            }
        }
        if !(quality.is_finite() && (0.0..=1.0).contains(&quality)) {
            quality = 0.0;
        }
        qualities.insert(coding, quality);
    }
    let wildcard = qualities.get("*").copied().unwrap_or(0.0);
    let mut best = None;
    let mut best_quality = 0.0;
    for encoding in ENCODINGS
        .into_iter()
        .filter(|encoding| available.contains(encoding))
    {
        let quality = qualities.get(encoding.token()).copied().unwrap_or(wildcard);
        if quality > best_quality {
            best = Some(encoding);
            best_quality = quality;
        }
    }
    best
}

/// Whether `If-None-Match` names `etag` (by weak comparison, as RFC 9110 asks) or is `*`.
fn none_match(headers: &HeaderMap, etag: &HeaderValue) -> bool {
    let etag = etag.to_str().unwrap_or("");
    header_list(headers, header::IF_NONE_MATCH)
        .split(',')
        .map(str::trim)
        .any(|tag| tag == "*" || tag.strip_prefix("W/").unwrap_or(tag) == etag)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOTH: [Encoding; 2] = [Encoding::Brotli, Encoding::Gzip];

    fn token(accept_encoding: &str, available: &[Encoding]) -> Option<&'static str> {
        preferred_encoding(accept_encoding, available).map(Encoding::token)
    }

    #[test]
    fn negotiates_like_the_dev_site_worker() {
        // tests/dev-site-worker.test.ts, with both copies available.
        assert_eq!(token("gzip, deflate, br, zstd", &BOTH), Some("br"));
        assert_eq!(token("br;q=0, gzip;q=0.5, zstd;q=1", &BOTH), Some("gzip"));
        assert_eq!(token("br;q=0, *;q=0.8", &BOTH), Some("gzip"));
        assert_eq!(token("br;q=0.2, gzip;q=0.8", &BOTH), Some("gzip"));
        assert_eq!(token("br;q=0.5, gzip;q=0.5", &BOTH), Some("br"));
        assert_eq!(token("br;q=invalid, gzip;q=1", &BOTH), Some("gzip"));
        assert_eq!(token("br ;q=0, gzip", &BOTH), Some("gzip"));
        assert_eq!(token("br;q=2, gzip;q=-1", &BOTH), None);
        assert_eq!(token("identity", &BOTH), None);
        assert_eq!(token("br;q=0, gzip;q=0, zstd;q=0", &BOTH), None);
        assert_eq!(token("", &BOTH), None);
    }

    #[test]
    fn negotiates_only_among_the_copies_a_file_has() {
        assert_eq!(token("br, gzip", &[Encoding::Gzip]), Some("gzip"));
        assert_eq!(token("br", &[Encoding::Gzip]), None);
        assert_eq!(token("gzip;q=0.1, br", &[Encoding::Brotli]), Some("br"));
        assert_eq!(token("BR;Q=0.4, *", &BOTH), Some("gzip"));
        assert_eq!(token("br, gzip", &[]), None);
    }

    #[test]
    fn compares_if_none_match_weakly_across_lists() {
        let etag = HeaderValue::from_static("\"0123456789abcdef-br\"");
        let matches = |value: &'static str| {
            let mut headers = HeaderMap::new();
            headers.insert(header::IF_NONE_MATCH, HeaderValue::from_static(value));
            none_match(&headers, &etag)
        };
        assert!(matches("\"0123456789abcdef-br\""));
        assert!(matches("W/\"0123456789abcdef-br\""));
        assert!(matches("\"other\", \"0123456789abcdef-br\""));
        assert!(matches("*"));
        assert!(!matches("\"0123456789abcdef\""));
        assert!(!none_match(&HeaderMap::new(), &etag));
    }

    #[test]
    fn writes_the_build_line_like_the_page() {
        let build = |release: Option<&str>, commit: Option<&str>, dirty| BuildInfo {
            release: release.map(str::to_string),
            commit: commit.map(str::to_string),
            dirty,
            built_at: None,
        };
        let full = build(Some("1.1.0.628"), Some("27e68e8"), false);
        assert_eq!(build_label(&full), "v1.1.0.628 · 27e68e8");
        let dirty = build(Some("1.1.1"), Some("27e68e8"), true);
        assert_eq!(build_label(&dirty), "v1.1.1 · 27e68e8+");
        assert_eq!(build_label(&build(Some("1.1.1"), None, false)), "v1.1.1");
        assert_eq!(build_label(&build(None, Some("27e68e8"), true)), "27e68e8+");
        assert_eq!(build_label(&BuildInfo::default()), "local build");
        assert_eq!(
            escape_html("v1<2> & \"3\" '4'"),
            "v1&lt;2&gt; &amp; &quot;3&quot; &#39;4&#39;"
        );
    }

    #[test]
    fn types_and_caches_by_path() {
        assert_eq!(content_type("index.html"), "text/html; charset=utf-8");
        assert_eq!(
            content_type("assets/game-dXnl6ALO.js"),
            "text/javascript; charset=utf-8"
        );
        assert_eq!(
            content_type("assets/engine_bg-cpClQGGO.wasm"),
            "application/wasm"
        );
        assert_eq!(
            content_type("manifest.webmanifest"),
            "application/manifest+json"
        );
        assert_eq!(content_type("textures/water/NORMALS.WEBP"), "image/webp");
        assert_eq!(content_type("audio/fire.mp3"), "audio/mpeg");
        assert_eq!(
            content_type("assets/no-extension"),
            "application/octet-stream"
        );
        let file = |path| site_file(path, b"bytes".to_vec(), "label", None, None);
        assert_eq!(file("assets/game-dXnl6ALO.js").cache_control, IMMUTABLE);
        assert_eq!(
            file("textures/ground/dry-grass.webp").cache_control,
            REVALIDATE
        );
        assert_eq!(file("index.html").cache_control, REVALIDATE);
        assert!(file("index.html").html && !file("favicon.svg").html);
    }

    #[test]
    fn fills_the_placeholder_and_recompresses_only_that_page() {
        let page = format!(
            "<p>{}</p>{}",
            BUILD_LABEL_PLACEHOLDER,
            "<i>padding</i>".repeat(40)
        );
        let stale = Some(b"stale".to_vec());
        let filled = site_file(
            "index.html",
            page.into_bytes(),
            "v1 · abc",
            stale.clone(),
            stale.clone(),
        );
        let text = std::str::from_utf8(&filled.identity.bytes).unwrap();
        assert!(text.starts_with("<p>v1 · abc</p>"));
        assert!(
            filled.brotli.is_none(),
            "the build's Brotli copy holds the placeholder"
        );
        let gzip = filled
            .gzip
            .as_ref()
            .expect("a gzip copy of the filled page");
        assert_ne!(gzip.bytes.as_ref(), b"stale");
        // A page without the placeholder keeps the build's copies.
        let plain = site_file(
            "other.html",
            b"<p>plain</p>".to_vec(),
            "v1",
            stale.clone(),
            stale,
        );
        assert_eq!(plain.brotli.unwrap().bytes.as_ref(), b"stale");
    }

    #[test]
    fn refuses_a_missing_or_incomplete_bundle() {
        let missing = Path::new("/nonexistent/sloppy-site");
        assert!(Site::load(missing, &BuildInfo::default()).is_err());
        let directory = std::env::temp_dir().join(format!("sloppy-site-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("favicon.svg"), "<svg/>").unwrap();
        let refused = Site::load(&directory, &BuildInfo::default());
        std::fs::remove_dir_all(&directory).unwrap();
        assert!(refused.err().unwrap().ends_with("has no index.html"));
    }
}
