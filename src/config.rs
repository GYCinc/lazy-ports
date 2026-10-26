// SPDX-License-Identifier: MIT

use cosmic::cosmic_config::{self, CosmicConfigEntry, cosmic_config_derive::CosmicConfigEntry};
use serde::{Deserialize, Serialize};

/// App-level configuration persisted via cosmic-config.
#[derive(Debug, Default, Clone, CosmicConfigEntry, Eq, PartialEq)]
#[version = 2]
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
    /// Theme mode: "" or "industrial" => skin (default); "system" =>
    /// follow COSMIC. Only the exact string "system" (case-insensitive,
    /// trimmed) opts out of the skin.
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

    /// True when the industrial skin is active. Empty (unset) means the
    /// skin: the applet ships skinned; "system" is the explicit opt-out.
    /// Only the exact string "system" (case-insensitive, trimmed) opts out.
    pub fn use_industrial(&self) -> bool {
        !self.theme_mode.trim().eq_ignore_ascii_case("system")
    }

    #[allow(dead_code)]
    /// Canonical persisted name for the active mode. The skin is written as
    /// "industrial" so pre-v2 configs and the post-v2 default agree.
    pub fn theme_mode_name(&self) -> &'static str {
        if self.use_industrial() { "industrial" } else { "system" }
    }

    #[allow(dead_code)]
    /// v1 -> v2 migration. The Listening Ports rail now starts collapsed, so a
    /// v1 `show_ports = true` (which was the shipped default) is reset once.
    /// Every other key carries over untouched.
    pub fn migrate_from_v1(v1: &Self) -> Self {
        let mut c = v1.clone();
        c.show_ports = false;
        c
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

    #[test]
    fn use_industrial_is_skin_first() {
        let mut c = Config::default();
        // empty => skin
        c.theme_mode = String::new();
        assert!(c.use_industrial());
        // legacy "industrial" => skin
        c.theme_mode = "industrial".to_string();
        assert!(c.use_industrial());
        // garbage => skin (only exact "system" opts out)
        c.theme_mode = "Dark".to_string();
        assert!(c.use_industrial());
        // whitespace trimmed
        c.theme_mode = "  InDuStRiAl  ".to_string();
        assert!(c.use_industrial());
        // exact "system" => not skin
        c.theme_mode = "system".to_string();
        assert!(!c.use_industrial());
        c.theme_mode = "  SYSTEM  ".to_string();
        assert!(!c.use_industrial());
    }

    #[test]
    fn theme_mode_name_canonicalises() {
        let mut c = Config::default();
        c.theme_mode = String::new();
        assert_eq!(c.theme_mode_name(), "industrial");
        c.theme_mode = "system".to_string();
        assert_eq!(c.theme_mode_name(), "system");
    }

    #[test]
    fn migrate_from_v1_resets_show_ports_only() {
        let mut v1 = Config::default();
        v1.show_ports = true;
        v1.services_json = "svc".to_string();
        v1.refresh_secs = 7;
        v1.ignored_json = "ig".to_string();
        v1.colloquial = true;
        v1.hide_system = true;
        v1.accent = "Amber".to_string();
        v1.theme_mode = "system".to_string();
        let v2 = Config::migrate_from_v1(&v1);
        assert!(!v2.show_ports, "show_ports must reset to false");
        assert_eq!(v2.services_json, "svc");
        assert_eq!(v2.refresh_secs, 7);
        assert_eq!(v2.ignored_json, "ig");
        assert!(v2.colloquial);
        assert!(v2.hide_system);
        assert_eq!(v2.accent, "Amber");
        assert_eq!(v2.theme_mode, "system");
    }
}
