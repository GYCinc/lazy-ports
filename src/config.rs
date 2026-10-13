// SPDX-License-Identifier: MIT

use cosmic::cosmic_config::{self, CosmicConfigEntry, cosmic_config_derive::CosmicConfigEntry};
use serde::{Deserialize, Serialize};

/// App-level configuration persisted via cosmic-config.
#[derive(Debug, Default, Clone, CosmicConfigEntry, Eq, PartialEq)]
#[version = 1]
pub struct Config {
    pub services_json: String,
    /// Polling interval in seconds (refresh mode). 0 means default.
    pub refresh_secs: u64,
    /// JSON array of process names hidden from the ports list.
    pub ignored_json: String,
    /// Colloquial nomenclature mode (a.k.a. pimp mode).
    pub colloquial: bool,
    /// Hide ports owned by other users (system daemons).
    pub hide_system: bool,
    /// Keep discovered listening ports section open.
    pub show_ports: bool,
    /// Accent scheme name; empty means the default emerald.
    /// Retained for backwards compat; the swatch picker is gone in
    /// favour of the system accent. Only honoured in industrial mode.
    pub accent: String,
    /// Theme mode: "system" (default, follows COSMIC) or "industrial"
    /// (trimmed legacy dark plates + LEDs). Empty means system.
    pub theme_mode: String,
}

impl Config {
    /// Processes ignored in the ports list.
    pub fn ignored_processes(&self) -> Vec<String> {
        serde_json::from_str(&self.ignored_json).unwrap_or_default()
    }

    /// Refresh mode: Realtime = 1s, Balanced = 2s, Low Resource = 10s.
    pub fn refresh_interval(&self) -> u64 {
        match self.refresh_secs {
            0 => 2,
            v => v,
        }
    }

    /// Persisted accent name, normalised to a known scheme.
    pub fn accent_name(&self) -> &str {
        let a = self.accent.trim();
        if a.is_empty() { "Emerald" } else { a }
    }

    /// True when the industrial look is selected or by default.
    /// Returns false only when explicitly set to "system".
    pub fn use_industrial(&self) -> bool {
        let mode = self.theme_mode.trim();
        if mode.is_empty() {
            true
        } else {
            !mode.eq_ignore_ascii_case("system")
        }
    }
}

/// A pinned local service (port of LocalPorts' ManagedServiceConfiguration).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ServiceConfig {
    pub id: String,
    pub name: String,
    pub address: String,
    pub start_command: Option<String>,
    pub working_dir: Option<String>,
    #[serde(default)]
    pub reserved: bool,
}

impl ServiceConfig {
    /// Start requires both a working directory and a start command.
    pub fn can_start(&self) -> bool {
        let cmd = self.start_command.as_deref().map(str::trim);
        let dir = self.working_dir.as_deref().map(str::trim);
        matches!(cmd, Some(c) if !c.is_empty()) && matches!(dir, Some(d) if !d.is_empty())
    }

    pub fn host_port(&self) -> Option<(String, u16)> {
        parse_address(&self.address).map(|(host, port, _, _)| (host, port))
    }
}

/// Parses `localhost:PORT` / `http://localhost:PORT/path` into
/// (host, port, path, scheme). The scheme is the lowercased text before
/// `://`, defaulting to `"http"` when no scheme is present.
pub fn parse_address(addr: &str) -> Option<(String, u16, String, String)> {
    let addr = addr.trim();
    let (scheme, rest) = match addr.split_once("://") {
        Some((s, r)) => (s.to_lowercase(), r),
        None => ("http".to_string(), addr),
    };
    let (hostport, path) = match rest.find('/') {
        Some(i) => (&rest[..i], rest[i..].to_string()),
        None => (rest, "/".to_string()),
    };
    if hostport.is_empty() {
        return None;
    }
    let colon = hostport.rfind(':')?;
    let port: u16 = hostport[colon + 1..].parse().ok()?;
    if port == 0 {
        return None;
    }
    let host = hostport[..colon]
        .trim_matches(|c| c == '[' || c == ']')
        .to_string();
    if host.is_empty() {
        return None;
    }
    Some((host, port, path, scheme))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_address_carries_scheme() {
        assert_eq!(
            parse_address("tcp://localhost:4455"),
            Some((
                "localhost".to_string(),
                4455,
                "/".to_string(),
                "tcp".to_string()
            ))
        );
    }

    #[test]
    fn parse_address_defaults_to_http_scheme() {
        assert_eq!(
            parse_address("localhost:3000"),
            Some((
                "localhost".to_string(),
                3000,
                "/".to_string(),
                "http".to_string()
            ))
        );
    }
}
