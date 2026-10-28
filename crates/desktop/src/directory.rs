//! The server directory (`src/net/server-address.ts`): which game servers run, the
//! content version each plays and their open rooms. Battle Setup's Multiplayer tab reads
//! it off the main thread every few seconds. Reading and checking the answer, room codes
//! and the length stepper are `sloppy-core`'s (`net::room_list`, `net::protocol`); this
//! fetches the answer and owns the directory's address.

use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::Duration;

use serde_json::Value;
pub use sloppy_core::net::protocol::{read_room_code, room_code, stepped_minutes};
// Only the UI snapshots build a server list by hand.
#[cfg(test)]
pub use sloppy_core::net::room_list::ServerListing;
use sloppy_core::net::room_list::split_address;
// The Multiplayer tab calls a listed room, with the server that hosts it, a `RoomListing`.
pub use sloppy_core::net::room_list::{HostedRoom as RoomListing, ROOM_SEATS, ServerList};

/// The production directory, unless the build names another (`SLOPPY_DIRECTORY_URL`).
pub const DEFAULT_DIRECTORY: &str = match option_env!("SLOPPY_DIRECTORY_URL") {
    Some(url) => url,
    None => "https://sloppy-tanks-directory.fridman.me",
};
/// The environment whose servers the app lists.
const ENVIRONMENT: &str = "production";
/// A directory request that has not answered by now has failed.
pub const LIST_TIMEOUT: Duration = Duration::from_secs(8);

fn is_local_host(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "[::1]")
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

/// A directory request running on its own thread.
pub struct DirectoryRequest {
    answer: Receiver<Result<ServerList, String>>,
}

impl DirectoryRequest {
    /// GET the servers listed at `base` (see [`servers_address`]).
    pub fn start(base: &str) -> Self {
        let (sender, answer) = channel();
        let address = servers_address(base);
        let local = is_local_directory(base);
        std::thread::spawn(move || {
            let result = address.and_then(|address| fetch(&address, local));
            let _ = sender.send(result);
        });
        Self { answer }
    }

    /// The answer once it has come: the list, or why there is none in words a player
    /// reads.
    pub fn answer(&self) -> Option<Result<ServerList, String>> {
        match self.answer.try_recv() {
            Ok(answer) => Some(answer),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(Err("Rooms unavailable".into())),
        }
    }
}

fn fetch(address: &str, local: bool) -> Result<ServerList, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(LIST_TIMEOUT))
        .build()
        .into();
    let mut response = agent.get(address).call().map_err(|error| match error {
        ureq::Error::Timeout(_) => "The room list timed out".to_owned(),
        ureq::Error::StatusCode(_) => "Rooms unavailable".to_owned(),
        _ => "Can't reach the game server".to_owned(),
    })?;
    let value: Value = response
        .body_mut()
        .read_json()
        .map_err(|_| "Rooms unavailable".to_owned())?;
    let list = ServerList::read(&value, local)?;
    if list.servers.is_empty() {
        return Err("No game server is running".into());
    }
    Ok(list)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sloppy_core::net::protocol::is_room_code;

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
        assert!(!is_local_directory(DEFAULT_DIRECTORY));
        assert!(servers_address(DEFAULT_DIRECTORY).is_ok());
    }

    #[test]
    fn a_new_room_code_comes_from_fresh_random_bytes() {
        assert!(is_room_code(&room_code(random_bytes())));
    }
}
