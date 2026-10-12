// SPDX-License-Identifier: MIT

//! First-run import of services from the CBar localports plugin config.

use crate::config::ServiceConfig;
use serde::Deserialize;

#[derive(Deserialize)]
struct CBarEntry {
    name: String,
    port: u16,
    cmd: String,
}

const FOOT_PREFIX: &str = "foot --hold bash -c '";

/// Maps a CBar command to (start_command, working_dir), unwrapping the
/// `foot --hold bash -c 'cd DIR && CMD'` wrapper.
fn split_command(cmd: &str) -> (Option<String>, Option<String>) {
    let cmd = cmd.trim();
    let inner = cmd
        .strip_prefix(FOOT_PREFIX)
        .map(|rest| rest.strip_suffix('\'').unwrap_or(rest))
        .unwrap_or(cmd);

    if let Some(rest) = inner.strip_prefix("cd ")
        && let Some(idx) = rest.find(" && ")
    {
        let dir = rest[..idx].trim_matches('\'').trim();
        let command = rest[idx + 4..].trim();
        return (
            (!command.is_empty()).then(|| command.to_string()),
            (!dir.is_empty()).then(|| dir.to_string()),
        );
    }

    (Some(inner.to_string()), None)
}

/// Loads services from CBar's localports config, or None if absent/invalid.
pub fn load_services() -> Option<Vec<ServiceConfig>> {
    let home = std::env::var("HOME").ok()?;
    let path = std::path::Path::new(&home).join(".config/cbar/localports.json");
    let raw = std::fs::read_to_string(path).ok()?;
    let entries: Vec<CBarEntry> = serde_json::from_str(&raw).ok()?;

    let mut services = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for entry in entries {
        if entry.port == 0 || !seen.insert(entry.port) {
            continue;
        }
        let (start_command, mut working_dir) = split_command(&entry.cmd);
        // cwd-independent commands (systemctl, flatpak, ...) still need a dir
        if working_dir.is_none() {
            working_dir = Some(home.clone());
        }
        services.push(ServiceConfig {
            id: format!("service-{}", entry.port),
            name: entry.name,
            address: format!("http://localhost:{}", entry.port),
            start_command,
            working_dir,
            reserved: false,
        });
    }
    (!services.is_empty()).then_some(services)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_foot_wrapper() {
        let (cmd, dir) = split_command(
            "foot --hold bash -c 'cd /home/hayron/MyProjects/The-Map && npm run dev -- --port 5247'",
        );
        assert_eq!(dir.as_deref(), Some("/home/hayron/MyProjects/The-Map"));
        assert_eq!(cmd.as_deref(), Some("npm run dev -- --port 5247"));
    }

    #[test]
    fn passes_plain_commands_through() {
        let (cmd, dir) = split_command("systemctl --user start hyprwhspr.service");
        assert_eq!(
            cmd.as_deref(),
            Some("systemctl --user start hyprwhspr.service")
        );
        assert_eq!(dir, None);
    }

    #[test]
    fn loads_real_cbar_config() {
        // Requires the real CBar config; validates the full mapping.
        if std::env::var("HOME")
            .map(|h| !h.is_empty())
            .unwrap_or(false)
            && std::path::Path::new(&std::env::var("HOME").unwrap())
                .join(".config/cbar/localports.json")
                .exists()
        {
            let services = load_services().expect("cbar config should load");
            assert_eq!(services.len(), 11);
            let git = services.iter().find(|s| s.name == "GitEnglish").unwrap();
            assert_eq!(git.address, "http://localhost:4005");
            assert_eq!(git.start_command.as_deref(), Some("bun run dev"));
            assert_eq!(
                git.working_dir.as_deref(),
                Some("/home/hayron/MyProjects/gitEnglishDD/gitenglish-admin")
            );
            assert!(services.iter().all(|s| s.can_start()));
        }
    }
}
