//! The server directory (`src/net/server-address.ts`): which game servers run, the
//! content version each plays and their open rooms. Battle Setup's Multiplayer tab reads
//! it beside the frames every few seconds. Reading and checking the answer, room codes
//! and the length stepper are `sloppy-core`'s (`net::room_list`, `net::protocol`); this
//! fetches the answer and owns the directory's address.

use std::sync::mpsc::TryRecvError;
use std::time::Duration;

use serde_json::Value;
use sloppy_core::net::room_list::{ServerList, split_address};

use crate::tasks::Background;

/// The directory the build names (`SLOPPY_DIRECTORY_URL`; empty, as CI passes for a pull
/// request, names none).
const BUILD_DIRECTORY: Option<&str> = match option_env!("SLOPPY_DIRECTORY_URL") {
    Some(url) if !url.is_empty() => Some(url),
    _ => None,
};
const PRODUCTION_DIRECTORY: &str = "https://directory.sloppytanks.com";
/// A game server on this machine (`pnpm run server:dev`), which is its own directory.
const LOCAL_SERVER: &str = "http://127.0.0.1:8787";
/// The environment whose servers the app lists: production's, unless the build names
/// another (`SLOPPY_SERVER_ENVIRONMENT`, `dev` for the dev site).
pub const ENVIRONMENT: &str = match option_env!("SLOPPY_SERVER_ENVIRONMENT") {
    Some(environment) if !environment.is_empty() => environment,
    _ => "production",
};
/// A directory request that has not answered by now has failed.
pub const LIST_TIMEOUT: Duration = Duration::from_secs(8);

pub fn is_local_host(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "[::1]")
}

/// The directory to list rooms from unless `--directory` names another: the build's,
/// else production's, or for a page on this machine the local server
/// (`resolveDirectoryAddress`).
pub fn default_directory() -> String {
    let local = if crate::links::local_page() {
        LOCAL_SERVER
    } else {
        PRODUCTION_DIRECTORY
    };
    BUILD_DIRECTORY.unwrap_or(local).to_owned()
}

/// The directory's `/servers` address for `base`: `http:` or `https:` without
/// credentials, query or fragment, and anything but a local server needs `https:`.
pub fn servers_address(base: &str) -> Result<String, String> {
    let invalid = || format!("Invalid server directory address: {base}");
    let (scheme, host) = split_address(base).ok_or_else(invalid)?;
    let local = is_local_host(host);
    if !(scheme == "https" || (scheme == "http" && local)) {
        return Err(invalid());
    }
    Ok(format!(
        "{}/servers?environment={ENVIRONMENT}",
        base.trim_end_matches('/')
    ))
}

/// Whether the directory at `base` is a local server, which may list `ws:` servers.
pub fn is_local_directory(base: &str) -> bool {
    split_address(base).is_some_and(|(_, host)| is_local_host(host))
}

/// Eight bytes from the standard library's per-process random hash keys.
#[cfg(not(target_arch = "wasm32"))]
pub fn random_bytes() -> [u8; 8] {
    use std::hash::{BuildHasher, Hasher};
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |time| time.as_nanos()),
    );
    hasher.finish().to_le_bytes()
}

/// Eight bytes from the browser's random draws (a page has no per-process hash keys).
#[cfg(target_arch = "wasm32")]
pub fn random_bytes() -> [u8; 8] {
    let draw = || (js_sys::Math::random() * f64::from(u32::MAX)) as u32;
    let (high, low) = (draw(), draw());
    (u64::from(high) << 32 | u64::from(low)).to_le_bytes()
}

const TIMED_OUT: &str = "The room list timed out";
const UNAVAILABLE: &str = "Rooms unavailable";
const UNREACHABLE: &str = "Can't reach the game server";

/// A directory request running beside the frames.
pub struct DirectoryRequest {
    answer: Background<Result<ServerList, String>>,
}

impl DirectoryRequest {
    /// GET the servers listed at `base` (see [`servers_address`]).
    pub fn start(base: &str) -> Self {
        let address = servers_address(base);
        let local = is_local_directory(base);
        let answer = Background::spawn(async move {
            let value = get_json(&address?).await?;
            let list = ServerList::read(&value, local)?;
            if list.servers.is_empty() {
                return Err("No game server is running".into());
            }
            Ok(list)
        });
        Self { answer }
    }

    /// The answer once it has come: the list, or why there is none in words a player
    /// reads.
    pub fn answer(&self) -> Option<Result<ServerList, String>> {
        match self.answer.answer() {
            Ok(answer) => Some(answer),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(Err(UNAVAILABLE.into())),
        }
    }
}

/// GET `address` as JSON, or why not in words a player reads; natively it blocks its
/// thread.
#[cfg(not(target_arch = "wasm32"))]
async fn get_json(address: &str) -> Result<Value, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(LIST_TIMEOUT))
        .build()
        .into();
    let mut response = agent.get(address).call().map_err(|error| match error {
        ureq::Error::Timeout(_) => TIMED_OUT.to_owned(),
        ureq::Error::StatusCode(_) => UNAVAILABLE.to_owned(),
        _ => UNREACHABLE.to_owned(),
    })?;
    response
        .body_mut()
        .read_json()
        .map_err(|_| UNAVAILABLE.to_owned())
}

/// GET `address` as JSON with the page's `fetch`, or why not in words a player reads.
#[cfg(target_arch = "wasm32")]
async fn get_json(address: &str) -> Result<Value, String> {
    use wasm_bindgen::{JsCast, JsValue};
    use wasm_bindgen_futures::JsFuture;
    let timed_out = |error: &JsValue| {
        js_sys::Reflect::get(error, &"name".into())
            .ok()
            .and_then(|name| name.as_string())
            .is_some_and(|name| name == "TimeoutError")
    };
    let window = web_sys::window().ok_or(UNREACHABLE)?;
    let init = web_sys::RequestInit::new();
    let timeout = web_sys::AbortSignal::timeout_with_u32(LIST_TIMEOUT.as_millis() as u32);
    init.set_signal(Some(&timeout));
    let response = JsFuture::from(window.fetch_with_str_and_init(address, &init))
        .await
        .map_err(|error| {
            if timed_out(&error) {
                TIMED_OUT
            } else {
                UNREACHABLE
            }
        })?;
    let response: web_sys::Response = response.dyn_into().map_err(|_| UNREACHABLE)?;
    if !response.ok() {
        return Err(UNAVAILABLE.into());
    }
    let text = JsFuture::from(response.text().map_err(|_| UNAVAILABLE)?)
        .await
        .map_err(|error| {
            if timed_out(&error) {
                TIMED_OUT
            } else {
                UNAVAILABLE
            }
        })?;
    serde_json::from_str(&text.as_string().unwrap_or_default()).map_err(|_| UNAVAILABLE.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_directory_needs_https_except_on_this_machine() {
        assert_eq!(
            servers_address("https://dir.example/").unwrap(),
            "https://dir.example/servers?environment=production"
        );
        assert_eq!(
            servers_address("http://127.0.0.1:8790").unwrap(),
            "http://127.0.0.1:8790/servers?environment=production"
        );
        assert_eq!(
            servers_address("http://[::1]:8790").unwrap(),
            "http://[::1]:8790/servers?environment=production"
        );
        assert!(servers_address("http://dir.example").is_err());
        assert!(servers_address("https://dir.example/?a=b").is_err());
        assert!(servers_address("https://user@dir.example").is_err());
        assert!(servers_address("ftp://localhost").is_err());
        assert!(is_local_directory("http://localhost:8790"));
        assert!(is_local_directory("http://[::1]:8790/"));
        // The app is no page on this machine: it lists a directory that needs `https:`.
        assert!(!is_local_directory(&default_directory()));
        assert!(servers_address(&default_directory()).is_ok());
        assert!(servers_address(LOCAL_SERVER).is_ok());
    }

    /// Players creating rooms at the same moment must not share a code, so every draw
    /// is new. Two honest draws of eight bytes agree about once in 2^64.
    #[test]
    fn a_new_room_code_comes_from_fresh_random_bytes() {
        assert_ne!(random_bytes(), random_bytes(), "a constant or reused seed");
    }
}
