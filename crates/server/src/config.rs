//! Settings from the environment (`server/main.ts`).

/// Local Vite and preview origins; deployments list their public sites in `ALLOWED_ORIGINS`.
const LOCAL_ORIGIN_PORTS: [u16; 5] = [5173, 5174, 5175, 4179, 4180];
const DEFAULT_HOST: &str = "127.0.0.1";
const DEFAULT_PORT: u16 = 8787;

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
    /// Reads `HOST`, `PORT`, `ALLOWED_ORIGINS`, `MAX_ROOMS` and `TRUST_PROXY`. A typo must
    /// stop startup, not silently turn a limit off.
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
}
