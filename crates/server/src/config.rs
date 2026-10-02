//! Settings from the environment.

/// Local Vite and preview origins; deployments list their public sites in `ALLOWED_ORIGINS`.
const LOCAL_ORIGIN_PORTS: [u16; 5] = [5173, 5174, 5175, 4179, 4180];
const DEFAULT_HOST: &str = "127.0.0.1";
const DEFAULT_PORT: u16 = 8787;
/// Commits show abbreviated, like the page's `/health`.
const SHORT_COMMIT_LENGTH: usize = 7;

/// What the server image was built from, for `/health`. The image sets it as
/// environment (`Dockerfile`) rather than the binary embedding it, so a new commit
/// that leaves the server unchanged does not recompile it. A local run has none.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BuildInfo {
    pub commit: Option<String>,
    /// The tree had uncommitted changes (`SLOPPY_COMMIT` ends in `-dirty`).
    pub dirty: bool,
    pub built_at: Option<String>,
}

impl BuildInfo {
    /// Reads `SLOPPY_COMMIT` (a full hash, `-dirty` when the tree had local changes)
    /// and `SLOPPY_BUILT_AT`; the image sets them empty when it was built without them.
    pub fn from_env(lookup: &impl Fn(&str) -> Option<String>) -> Self {
        let present = |name| lookup(name).filter(|text: &String| !text.is_empty());
        let stamp = present("SLOPPY_COMMIT");
        let (hash, dirty) = match stamp.as_deref().map(|text| text.strip_suffix("-dirty")) {
            Some(Some(hash)) => (Some(hash), true),
            Some(None) => (stamp.as_deref(), false),
            None => (None, false),
        };
        Self {
            commit: hash.map(|hash| hash.chars().take(SHORT_COMMIT_LENGTH).collect()),
            dirty,
            built_at: present("SLOPPY_BUILT_AT"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    pub host: String,
    pub port: u16,
    /// Exact origins allowed to list rooms and open room sockets.
    pub allowed_origins: Vec<String>,
    /// `MAX_ROOMS`, when set.
    pub max_rooms: Option<usize>,
    /// Take the client IP from the last `X-Forwarded-For` hop (the local reverse proxy).
    pub trust_proxy: bool,
    pub build: BuildInfo,
}

pub fn local_origins() -> Vec<String> {
    LOCAL_ORIGIN_PORTS
        .iter()
        .flat_map(|port| {
            [
                format!("http://127.0.0.1:{port}"),
                format!("http://localhost:{port}"),
            ]
        })
        .collect()
}

impl Settings {
    /// Reads `HOST`, `PORT`, `ALLOWED_ORIGINS`, `MAX_ROOMS`, `TRUST_PROXY` and the
    /// [`BuildInfo`] stamps. A typo must stop startup, not silently turn a limit off.
    pub fn from_env(lookup: impl Fn(&str) -> Option<String>) -> Result<Self, String> {
        let host = lookup("HOST").unwrap_or_else(|| DEFAULT_HOST.to_string());
        let port = match lookup("PORT") {
            None => DEFAULT_PORT,
            Some(text) => text
                .trim()
                .parse()
                .map_err(|_| "PORT must be a port number".to_string())?,
        };
        let loopback = matches!(host.as_str(), "127.0.0.1" | "::1" | "localhost");
        let origins: Vec<String> = lookup("ALLOWED_ORIGINS")
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|origin| !origin.is_empty())
            .map(str::to_string)
            .collect();
        let max_rooms = match lookup("MAX_ROOMS").filter(|text| !text.is_empty()) {
            None => None,
            Some(text) => match text.parse::<usize>() {
                Ok(value) if value >= 1 => Some(value),
                _ => return Err("MAX_ROOMS must be a positive integer".into()),
            },
        };
        // Only a loopback listener can be sure its X-Forwarded-For came from the local proxy.
        let trust_proxy = lookup("TRUST_PROXY").unwrap_or_else(|| loopback.to_string()) == "true";
        Ok(Self {
            host,
            port,
            allowed_origins: if origins.is_empty() {
                local_origins()
            } else {
                origins
            },
            max_rooms,
            trust_proxy,
            build: BuildInfo::from_env(&lookup),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn settings(pairs: &[(&str, &str)]) -> Result<Settings, String> {
        let env: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        Settings::from_env(|name| env.get(name).cloned())
    }

    #[test]
    fn defaults_to_a_loopback_listener_trusting_its_proxy() {
        let settings = settings(&[]).unwrap();
        assert_eq!((settings.host.as_str(), settings.port), ("127.0.0.1", 8787));
        assert!(settings.trust_proxy);
        assert_eq!(settings.max_rooms, None);
        assert!(
            settings
                .allowed_origins
                .contains(&"http://localhost:5173".to_string())
        );
        assert_eq!(settings.allowed_origins.len(), 10);
    }

    #[test]
    fn reads_overrides_and_rejects_typos() {
        let custom = settings(&[
            ("HOST", "0.0.0.0"),
            ("PORT", "9000"),
            ("ALLOWED_ORIGINS", " https://a.example , https://b.example,"),
            ("MAX_ROOMS", "3"),
        ])
        .unwrap();
        assert_eq!(custom.port, 9000);
        assert!(
            !custom.trust_proxy,
            "a public listener cannot trust X-Forwarded-For"
        );
        assert_eq!(
            custom.allowed_origins,
            ["https://a.example", "https://b.example"]
        );
        assert_eq!(custom.max_rooms, Some(3));
        assert!(
            settings(&[("HOST", "0.0.0.0"), ("TRUST_PROXY", "true")])
                .unwrap()
                .trust_proxy
        );
        assert_eq!(settings(&[("MAX_ROOMS", "")]).unwrap().max_rooms, None);
        assert!(settings(&[("MAX_ROOMS", "0")]).is_err());
        assert!(settings(&[("MAX_ROOMS", "ten")]).is_err());
        assert!(settings(&[("PORT", "http")]).is_err());
    }

    #[test]
    fn reads_the_image_build_stamps() {
        assert_eq!(settings(&[]).unwrap().build, BuildInfo::default());
        let clean = settings(&[
            ("SLOPPY_COMMIT", "27e68e8f7c62b742b2efb24987016bf08778e9d9"),
            ("SLOPPY_BUILT_AT", "2026-10-02T14:02:23.799Z"),
        ])
        .unwrap()
        .build;
        assert_eq!(clean.commit.as_deref(), Some("27e68e8"));
        assert!(!clean.dirty);
        assert_eq!(clean.built_at.as_deref(), Some("2026-10-02T14:02:23.799Z"));
        let dirty = settings(&[
            ("SLOPPY_COMMIT", "27e68e8f7c62-dirty"),
            ("SLOPPY_BUILT_AT", ""),
        ])
        .unwrap()
        .build;
        assert_eq!(dirty.commit.as_deref(), Some("27e68e8"));
        assert!(dirty.dirty);
        assert_eq!(dirty.built_at, None);
    }
}
