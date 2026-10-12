// SPDX-License-Identifier: MIT

use cosmic::cosmic_config::{self, cosmic_config_derive::CosmicConfigEntry, CosmicConfigEntry};
use serde::{Deserialize, Serialize};

/// App-level configuration persisted via cosmic-config.
#[derive(Debug, Default, Clone, CosmicConfigEntry, Eq, PartialEq)]
#[version = 1]
pub struct Config {
    pub services_json: String,
    pub refresh_secs: u64,
    pub ignored_json: String,
    pub colloquial: bool,
    pub hide_system: bool,
    pub show_ports: bool,
}

impl Config {
    pub fn ignored_processes(&self) -> Vec<String> {
        serde_json::from_str(&self.ignored_json).unwrap_or_default()
    }

    pub fn refresh_interval(&self) -> u64 {
        match self.refresh_secs {
            0 => 2,
            v => v,
        }
    }
}

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
    pub fn can_start(&self) -> bool {
        let cmd = self.start_command.as_deref().map(str::trim);
        let dir = self.working_dir.as_deref().map(str::trim);
        matches!(cmd, Some(c) if !c.is_empty()) && matches!(dir, Some(d) if !d.is_empty())
    }

    pub fn host_port(&self) -> Option<(String, u16)> {
        parse_address(&self.address).map(|(host, port, _, _)| (host, port))
    }
}

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
            Some(("localhost".to_string(), 4455, "/".to_string(), "tcp".to_string()))
        );
    }

    #[test]
    fn parse_address_defaults_to_http_scheme() {
        assert_eq!(
            parse_address("localhost:3000"),
            Some(("localhost".to_string(), 3000, "/".to_string(), "http".to_string()))
        );
    }
}
