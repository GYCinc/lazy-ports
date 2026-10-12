// SPDX-License-Identifier: MIT

use std::collections::HashSet;

/// A locally listening TCP port (port of LocalPorts' ListeningPort).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListeningPort {
    pub pid: i32,
    pub process: String,
    pub display_name: String,
    pub framework: Option<String>,
    pub port: u16,
    pub host: String,
    pub wildcard: bool,
    /// Owned by a different user (system daemon) — hideable via the toggle.
    pub is_system: bool,
}

impl ListeningPort {
    pub fn new(pid: i32, process: String, port: u16, host: String, wildcard: bool, is_system: bool) -> Self {
        Self {
            display_name: process.clone(),
            framework: None,
            pid,
            process,
            port,
            host,
            wildcard,
            is_system,
        }
    }
}

/// Scans listening TCP ports via lsof (as LocalPorts does) with an `ss` fallback.
pub async fn scan() -> Vec<ListeningPort> {
    let mut all = Vec::new();

    if let Ok(res) = tokio::process::Command::new("lsof")
        .args(["-nP", "+c0", "-iTCP", "-sTCP:LISTEN"])
        .output()
        .await
        && res.status.success()
    {
        all.extend(parse_lsof(&String::from_utf8_lossy(&res.stdout)));
    }

    if let Ok(res) = tokio::process::Command::new("ss")
        .arg("-tlnp")
        .output()
        .await
        && res.status.success()
    {
        all.extend(parse_ss(&String::from_utf8_lossy(&res.stdout)));
    }

    // Deduplicate: if multiple processes hold the same port (e.g. child process
    // inheriting an open socket file descriptor), attribute the port to the primary process (lowest PID).
    all.sort_by_key(|p| (p.port, p.pid));
    let mut seen = HashSet::new();
    let mut ports: Vec<ListeningPort> = all.into_iter().filter(|p| seen.insert(p.port)).collect();
    ports.sort_by_key(|p| (p.port, p.pid));

    // Ownership: /proc/<pid> dir owner vs our own euid.
    use std::os::unix::fs::MetadataExt;
    let current_uid = std::fs::metadata("/proc/self")
        .map(|m| m.uid())
        .unwrap_or(u32::MAX);
    for p in &mut ports {
        p.is_system = std::fs::metadata(format!("/proc/{}", p.pid))
            .map(|m| m.uid() != current_uid)
            .unwrap_or(false);

        let (resolved_name, framework) = inspect_process_meta(p.pid, &p.process);
        p.display_name = resolved_name.unwrap_or_else(|| p.process.clone());
        p.framework = framework;
    }

    // Exclude Lazy Ports' own process so its internal reservation socket listeners
    // are not reported as external running services or discovered ports.
    let my_pid = std::process::id() as i32;
    ports.retain(|p| p.pid != my_pid && p.process != "lazy-ports" && p.process != "lopee");

    ports
}

/// Inspects the process environment (/proc/<pid>/cwd, /proc/<pid>/cmdline) to identify
/// the real project name and framework/runtime.
pub fn inspect_process_meta(pid: i32, process_name: &str) -> (Option<String>, Option<String>) {
    let cwd = process_cwd(pid);
    let cmdline = process_cmdline(pid);

    let mut project_name: Option<String> = None;
    let mut framework: Option<String> = None;

    // 1. Check CWD for project manifests
    if let Some(ref dir) = cwd {
        let path = std::path::Path::new(dir);
        let is_generic = dir == "/"
            || dir == "/root"
            || dir == "/usr/bin"
            || dir.starts_with("/tmp")
            || dir == "/home/hayron";

        if !is_generic {
            // Check package.json
            let pkg_json = path.join("package.json");
            if let Ok(content) = std::fs::read_to_string(&pkg_json) {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(name) = val.get("name").and_then(|v| v.as_str()) {
                        if !name.is_empty() {
                            project_name = Some(name.to_string());
                        }
                    }
                }
            }

            // Check Cargo.toml
            if project_name.is_none() {
                let cargo_toml = path.join("Cargo.toml");
                if let Ok(content) = std::fs::read_to_string(&cargo_toml) {
                    for line in content.lines() {
                        let trimmed = line.trim();
                        if trimmed.starts_with("name") && trimmed.contains('=') {
                            if let Some(n) = trimmed.split('=').nth(1) {
                                let cleaned = n.trim().trim_matches('"').trim_matches('\'').trim();
                                if !cleaned.is_empty() {
                                    project_name = Some(cleaned.to_string());
                                    break;
                                }
                            }
                        }
                    }
                }
            }

            // Check pyproject.toml
            if project_name.is_none() {
                let pyproject = path.join("pyproject.toml");
                if let Ok(content) = std::fs::read_to_string(&pyproject) {
                    for line in content.lines() {
                        let trimmed = line.trim();
                        if (trimmed.starts_with("name") || trimmed.starts_with("project.name")) && trimmed.contains('=') {
                            if let Some(n) = trimmed.split('=').nth(1) {
                                let cleaned = n.trim().trim_matches('"').trim_matches('\'').trim();
                                if !cleaned.is_empty() {
                                    project_name = Some(cleaned.to_string());
                                    break;
                                }
                            }
                        }
                    }
                }
            }

            // Fallback: directory folder name
            if project_name.is_none() {
                if let Some(dir_name) = path.file_name().and_then(|s| s.to_str()) {
                    if !dir_name.is_empty() && dir_name != "home" {
                        project_name = Some(dir_name.to_string());
                    }
                }
            }
        }
    }

    // 2. Check cmdline for framework or script details
    if let Some(ref cmd) = cmdline {
        let lower = cmd.to_lowercase();
        if lower.contains("vite") {
            framework = Some("VITE".to_string());
        } else if lower.contains("next") {
            framework = Some("NEXT.JS".to_string());
        } else if lower.contains("nuxt") {
            framework = Some("NUXT".to_string());
        } else if lower.contains("astro") {
            framework = Some("ASTRO".to_string());
        } else if lower.contains("svelte") {
            framework = Some("SVELTE".to_string());
        } else if lower.contains("webpack") {
            framework = Some("WEBPACK".to_string());
        } else if lower.contains("uvicorn") {
            framework = Some("UVICORN".to_string());
        } else if lower.contains("flask") {
            framework = Some("FLASK".to_string());
        } else if lower.contains("fastapi") {
            framework = Some("FASTAPI".to_string());
        } else if lower.contains("django") {
            framework = Some("DJANGO".to_string());
        } else if lower.contains("http.server") {
            framework = Some("HTTP.SERVER".to_string());
        } else if lower.contains("streamlit") {
            framework = Some("STREAMLIT".to_string());
        } else if lower.contains("jupyter") {
            framework = Some("JUPYTER".to_string());
        } else if lower.contains("cargo") {
            framework = Some("CARGO".to_string());
        } else if lower.contains("docker") {
            framework = Some("DOCKER".to_string());
        } else if lower.contains("postgres") {
            framework = Some("POSTGRES".to_string());
        } else if lower.contains("redis") {
            framework = Some("REDIS".to_string());
        } else if process_name == "agy" {
            framework = Some("AGY".to_string());
        }

        // If project_name is still none, try extracting python script name / module name
        if project_name.is_none() {
            let parts: Vec<&str> = cmd.split_whitespace().collect();
            for (i, part) in parts.iter().enumerate() {
                if *part == "-m" && i + 1 < parts.len() {
                    let mod_name = parts[i + 1];
                    if mod_name != "http.server" {
                        project_name = Some(mod_name.to_string());
                    }
                    break;
                } else if part.ends_with(".py") || part.ends_with(".js") || part.ends_with(".ts") {
                    let file_name = std::path::Path::new(part)
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or(part);
                    if file_name != "index" && file_name != "main" && file_name != "server" {
                        project_name = Some(file_name.to_string());
                        break;
                    }
                }
            }
        }
    }

    if framework.is_none() {
        if process_name == "node" {
            framework = Some("NODE".to_string());
        } else if process_name == "python3" || process_name == "python" {
            framework = Some("PYTHON3".to_string());
        }
    }

    (project_name, framework)
}

/// Reads the command line of a process from /proc/<pid>/cmdline.
pub fn process_cmdline(pid: i32) -> Option<String> {
    if pid <= 0 {
        return None;
    }
    let bytes = std::fs::read(format!("/proc/{pid}/cmdline")).ok()?;
    if bytes.is_empty() {
        return None;
    }
    let cmd = String::from_utf8_lossy(&bytes)
        .replace('\0', " ")
        .trim()
        .to_string();
    if cmd.is_empty() {
        None
    } else {
        Some(cmd)
    }
}

/// Reads the working directory of a process from /proc/<pid>/cwd symlink.
pub fn process_cwd(pid: i32) -> Option<String> {
    if pid <= 0 {
        return None;
    }
    std::fs::read_link(format!("/proc/{pid}/cwd"))
        .ok()
        .and_then(|p| p.to_str().map(String::from))
}

/// A collection of listening ports belonging to the same process name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessGroup<'a> {
    pub process: String,
    pub display_name: String,
    pub framework: Option<String>,
    pub pids: Vec<i32>,
    pub ports: Vec<&'a ListeningPort>,
}

impl<'a> ProcessGroup<'a> {
    #[allow(dead_code)]
    pub fn primary_pid(&self) -> i32 {
        self.pids.first().copied().unwrap_or(0)
    }

    #[allow(dead_code)]
    pub fn formatted_ports(&self) -> String {
        self.ports
            .iter()
            .map(|p| format!(":{}", p.port))
            .collect::<Vec<_>>()
            .join(", ")
    }

    #[allow(dead_code)]
    pub fn formatted_pids(&self) -> String {
        self.pids
            .iter()
            .map(|pid| format!("pid {pid}"))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// Groups ports by process name and project name, sorted by first port.
#[allow(dead_code)]
pub fn group_by_process<'a>(ports: &'a [ListeningPort]) -> Vec<ProcessGroup<'a>> {
    let mut groups: Vec<ProcessGroup<'a>> = Vec::new();
    for p in ports {
        if let Some(entry) = groups.iter_mut().find(|g| g.process == p.process && g.display_name == p.display_name) {
            if !entry.pids.contains(&p.pid) {
                entry.pids.push(p.pid);
            }
            if !entry.ports.iter().any(|existing| existing.port == p.port) {
                entry.ports.push(p);
            }
        } else {
            groups.push(ProcessGroup {
                process: p.process.clone(),
                display_name: p.display_name.clone(),
                framework: p.framework.clone(),
                pids: vec![p.pid],
                ports: vec![p],
            });
        }
    }
    groups.sort_by_key(|g| g.ports.first().map(|p| p.port).unwrap_or(0));
    groups
}

/// Same as `group_by_process` but accepts a pre-filtered slice of references.
pub fn group_by_process_refs<'a>(ports: &[&'a ListeningPort]) -> Vec<ProcessGroup<'a>> {
    let mut groups: Vec<ProcessGroup<'a>> = Vec::new();
    for p in ports {
        if let Some(entry) = groups.iter_mut().find(|g| g.process == p.process && g.display_name == p.display_name) {
            if !entry.pids.contains(&p.pid) {
                entry.pids.push(p.pid);
            }
            if !entry.ports.iter().any(|existing| existing.port == p.port) {
                entry.ports.push(p);
            }
        } else {
            groups.push(ProcessGroup {
                process: p.process.clone(),
                display_name: p.display_name.clone(),
                framework: p.framework.clone(),
                pids: vec![p.pid],
                ports: vec![p],
            });
        }
    }
    groups.sort_by_key(|g| g.ports.first().map(|p| p.port).unwrap_or(0));
    groups
}

fn parse_lsof(output: &str) -> Vec<ListeningPort> {
    let mut ports = Vec::new();
    for line in output.lines() {
        if line.starts_with("COMMAND") || line.trim().is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 9 {
            continue;
        }
        let Ok(pid) = fields[1].parse::<i32>() else {
            continue;
        };
        let process = fields[0].to_string();
        let name = fields[8];
        let Some((host, port)) = split_host_port(name) else {
            continue;
        };
        let is_wild = is_wildcard(&host);
        ports.push(ListeningPort::new(
            pid,
            process,
            port,
            host,
            is_wild,
            false,
        ));
    }
    ports
}

fn parse_ss(output: &str) -> Vec<ListeningPort> {
    let mut ports = Vec::new();
    for line in output.lines().skip(1) {
        if line.trim().is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 4 || fields[0] != "LISTEN" {
            continue;
        }
        let Some((host, port)) = split_host_port(fields[3]) else {
            continue;
        };
        let mut process = String::new();
        let mut pid: i32 = -1;
        if let Some(start) = line.find("users:((") {
            let rest = &line[start + 8..];
            if let Some(end) = rest.find("))") {
                let inner = &rest[..end];
                let mut parts = inner.split(',');
                if let Some(first) = parts.next() {
                    process = first.trim_matches('"').to_string();
                }
                if let Some(pid_part) = parts.find_map(|kv| kv.trim().strip_prefix("pid=")) {
                    pid = pid_part.trim().parse().unwrap_or(-1);
                }
            }
        }
        if pid < 0 {
            continue;
        }
        let is_wild = is_wildcard(&host);
        ports.push(ListeningPort::new(
            pid,
            process,
            port,
            host,
            is_wild,
            false,
        ));
    }
    ports
}

fn split_host_port(name: &str) -> Option<(String, u16)> {
    let colon = name.rfind(':')?;
    let port: u16 = name[colon + 1..].parse().ok()?;
    Some((name[..colon].to_string(), port))
}

fn is_wildcard(host: &str) -> bool {
    let h = host.trim_matches(|c| c == '[' || c == ']');
    matches!(h, "*" | "0.0.0.0" | "::")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_ports_by_process_name_across_multiple_pids() {
        let ports = vec![
            ListeningPort::new(
                100,
                "chrome".to_string(),
                9655,
                "127.0.0.1".to_string(),
                false,
                false,
            ),
            ListeningPort::new(
                200,
                "chrome".to_string(),
                9622,
                "127.0.0.1".to_string(),
                false,
                false,
            ),
            ListeningPort::new(
                300,
                "node".to_string(),
                3000,
                "0.0.0.0".to_string(),
                true,
                false,
            ),
        ];

        let groups = group_by_process(&ports);
        assert_eq!(groups.len(), 2);
        let chrome_grp = groups.iter().find(|g| g.process == "chrome").unwrap();
        assert_eq!(chrome_grp.pids, vec![100, 200]);
        assert_eq!(chrome_grp.ports.len(), 2);
        assert_eq!(chrome_grp.formatted_ports(), ":9655, :9622");
        assert_eq!(chrome_grp.primary_pid(), 100);
    }
}
