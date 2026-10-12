// SPDX-License-Identifier: MIT

use crate::config::ServiceConfig;
use cosmic::iced::futures::channel::mpsc;
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::process::Stdio;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

/// Events emitted by the process manager (child exits).
#[derive(Debug, Clone)]
pub enum Event {
    Exited { id: String, code: Option<i32> },
}

static EVENT_TX: OnceLock<mpsc::UnboundedSender<Event>> = OnceLock::new();

pub fn set_event_tx(tx: mpsc::UnboundedSender<Event>) -> Result<(), mpsc::UnboundedSender<Event>> {
    EVENT_TX.set(tx)
}

fn send_event(ev: Event) {
    if let Some(tx) = EVENT_TX.get() {
        let _ = tx.unbounded_send(ev);
    }
}

/// pgid of the process group running each managed service.
static REGISTRY: OnceLock<Mutex<HashMap<String, u32>>> = OnceLock::new();

fn registry() -> &'static Mutex<HashMap<String, u32>> {
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Active port reservations holding ports for stopped services.
static RESERVATIONS: OnceLock<Mutex<HashMap<u16, std::net::TcpListener>>> = OnceLock::new();

fn reservations() -> &'static Mutex<HashMap<u16, std::net::TcpListener>> {
    RESERVATIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Reserves a port by binding a dummy TcpListener, preventing other processes from binding it.
pub fn reserve_port(port: u16) -> Result<(), String> {
    let mut map = reservations().lock().unwrap();
    if map.contains_key(&port) {
        return Ok(());
    }
    match std::net::TcpListener::bind(("127.0.0.1", port)) {
        Ok(listener) => {
            map.insert(port, listener);
            Ok(())
        }
        Err(e) => Err(format!("Could not reserve port {port}: {e}")),
    }
}

/// Releases a port reservation so a service can bind it.
pub fn release_port_reservation(port: u16) {
    if let Ok(mut map) = reservations().lock() {
        map.remove(&port);
    }
}

/// Checks if a port is currently reserved by Lazy Ports.
pub fn is_port_reserved(port: u16) -> bool {
    reservations().lock().unwrap().contains_key(&port)
}

/// Synchronizes reservations with configured services:
/// - If a service has `reserved: true` and is NOT running, reserve its port.
/// - If running or unreserved, ensure reservation is released.
pub fn sync_reservations(services: &[ServiceConfig]) {
    for svc in services {
        let Some((_, port)) = svc.host_port() else {
            continue;
        };
        let is_running = is_managed(&svc.id);
        if svc.reserved && !is_running {
            let _ = reserve_port(port);
        } else {
            release_port_reservation(port);
        }
    }
}

/// Per-id lifecycle locks, serializing start/stop/restart per service id
/// (ADR 0002). The lock is acquired inside the operation future, never
/// across Task boundaries.
static OP_LOCKS: OnceLock<Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>> = OnceLock::new();

async fn op_lock(id: &str) -> tokio::sync::OwnedMutexGuard<()> {
    let arc = {
        let map = OP_LOCKS.get_or_init(|| Mutex::new(HashMap::new()));
        map.lock()
            .unwrap()
            .entry(id.to_string())
            .or_default()
            .clone()
    };
    arc.lock_owned().await
}

/// Health of a service address, polled like LocalPorts' health checks.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq)]
pub enum Health {
    Unknown,
    Healthy,
    Unhealthy(u16),
    Failed(String),
    Unchecked,
}

pub fn log_dir() -> std::path::PathBuf {
    let base = std::env::var("XDG_DATA_HOME")
        .ok()
        .filter(|s| !s.is_empty())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
            std::path::PathBuf::from(home).join(".local/share")
        });
    base.join("lazy-ports").join("logs")
}

pub fn log_file_path(id: &str) -> std::path::PathBuf {
    log_dir().join(format!("{id}.log"))
}

/// Spawns a managed service under its own process group via the user's shell.
/// Logs are appended to the service's log file. Returns the pgid.
/// Serialized per service id (ADR 0002).
pub async fn spawn_managed(cfg: ServiceConfig) -> Result<u32, String> {
    let _g = op_lock(&cfg.id).await;
    spawn_unlocked(cfg).await
}

async fn spawn_unlocked(cfg: ServiceConfig) -> Result<u32, String> {
    let existing = { registry().lock().unwrap().get(&cfg.id).copied() };
    if let Some(pgid) = existing {
        if group_alive(pgid).await {
            return Ok(pgid);
        }
        registry().lock().unwrap().remove(&cfg.id);
    }

    let dir = cfg.working_dir.clone().unwrap_or_default();
    let dir_path = std::path::PathBuf::from(&dir);
    if dir.trim().is_empty() || !dir_path.is_dir() {
        return Err(format!("Project folder not found: {dir}"));
    }

    let cmd = cfg.start_command.clone().unwrap_or_default();
    if cmd.trim().is_empty() {
        return Err(format!("Start is not configured for {}", cfg.name));
    }

    let log_path = log_file_path(&cfg.id);
    std::fs::create_dir_all(log_dir()).map_err(|e| format!("log dir: {e}"))?;
    if let Ok(meta) = std::fs::metadata(&log_path)
        && meta.len() > 1_048_576
    {
        let _ = std::fs::rename(&log_path, log_path.with_extension("log.old"));
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(&log_path)
        .map_err(|e| format!("log file: {e}"))?;
    let banner = format!("\n=== started: {cmd} ===\n");
    file.write_all(banner.as_bytes())
        .map_err(|e| format!("log write: {e}"))?;
    drop(file);

    let stdout_file = std::fs::OpenOptions::new()
        .append(true)
        .open(&log_path)
        .map_err(|e| format!("log file: {e}"))?;
    let stderr_file = std::fs::OpenOptions::new()
        .append(true)
        .open(&log_path)
        .map_err(|e| format!("log file: {e}"))?;

    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".into());
    let mut command = tokio::process::Command::new(&shell);
    command
        .arg("-lc")
        .arg(&cmd)
        .current_dir(&dir_path)
        .stdout(stdout_file)
        .stderr(stderr_file)
        .stdin(Stdio::null())
        .process_group(0);

    if let Some((_, port)) = cfg.host_port() {
        release_port_reservation(port);
    }

    let mut child = command
        .spawn()
        .map_err(|e| format!("failed to start {}: {e}", cfg.name))?;
    let pgid = child.id().unwrap_or(0);

    let id = cfg.id.clone();
    tokio::spawn(async move {
        let code = child.wait().await.ok().and_then(|s| s.code());
        if let Ok(mut map) = registry().lock()
            && map.get(&id) == Some(&pgid)
        {
            map.remove(&id);
        }
        send_event(Event::Exited { id, code });
    });

    registry().lock().unwrap().insert(cfg.id.clone(), pgid);
    Ok(pgid)
}

/// SIGTERMs a process group, waits up to 3s, then SIGKILLs it.
async fn terminate_pgid(pgid: u32) {
    signal_group(pgid, "TERM").await;
    for _ in 0..30 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if !group_alive(pgid).await {
            break;
        }
    }
    signal_group(pgid, "KILL").await;
}

/// Stops a managed service: SIGTERM to the process group, then SIGKILL after 3s.
/// Serialized per service id (ADR 0002).
pub async fn stop_managed(id: String) -> Result<(), String> {
    let _g = op_lock(&id).await;
    stop_unlocked(&id).await.map(|_| ())
}

/// Unlocked stop path. Returns whether an incarnation was terminated.
async fn stop_unlocked(id: &str) -> Result<bool, String> {
    let Some(pgid) = registry().lock().unwrap().get(id).copied() else {
        return Ok(false);
    };
    terminate_pgid(pgid).await;
    if let Ok(mut map) = registry().lock() {
        map.remove(id);
    }
    Ok(true)
}

/// Restarts a managed service: stop any previous incarnation, then start
/// again. Returns the new pgid and whether an old incarnation was terminated.
/// Takes the per-id lock once across the stop + spawn pair (ADR 0002).
pub async fn restart_managed(cfg: ServiceConfig) -> Result<(u32, bool), String> {
    let _g = op_lock(&cfg.id).await;
    let stopped_old = stop_unlocked(&cfg.id).await?;
    let pgid = spawn_unlocked(cfg).await?;
    Ok((pgid, stopped_old))
}

/// Whether the registry currently tracks a process group for `id`.
pub fn is_managed(id: &str) -> bool {
    registry().lock().unwrap().contains_key(id)
}

/// Kills an arbitrary PID (quit a listening process from the ports list).
pub async fn kill_pid(pid: i32) -> Result<(), String> {
    if pid <= 1 {
        return Err("Refusing to kill this PID".into());
    }
    let _ = tokio::process::Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .status()
        .await;
    for _ in 0..20 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let alive = tokio::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .status()
            .await
            .map(|s| s.success())
            .unwrap_or(true);
        if !alive {
            return Ok(());
        }
    }
    let _ = tokio::process::Command::new("kill")
        .args(["-KILL", &pid.to_string()])
        .status()
        .await;
    Ok(())
}

async fn signal_group(pgid: u32, sig: &str) {
    let arg = format!("-{pgid}");
    let sig_arg = format!("-{sig}");
    let _ = tokio::process::Command::new("kill")
        .args([&sig_arg, &arg])
        .status()
        .await;
}

async fn group_alive(pgid: u32) -> bool {
    let arg = format!("-{pgid}");
    tokio::process::Command::new("kill")
        .args(["-0", &arg])
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(true)
}

/// Desktop notification when a service dies unexpectedly (notify-send).
pub async fn notify_crash(name: String, code: Option<i32>) {
    tokio::task::spawn_blocking(move || {
        let body = match code {
            Some(c) => format!("Exit code {c}"),
            None => "Killed by signal".to_string(),
        };
        let _ = std::process::Command::new("notify-send")
            .args([
                "-a",
                "Lazy Ports",
                "-i",
                "network-server-symbolic",
                &format!("{name} stopped unexpectedly"),
                &body,
            ])
            .status();
    })
    .await
    .ok();
}

/// Reads the last 32 KiB of a service's log file for the log viewer.
pub async fn read_tail(path: String) -> String {
    tokio::task::spawn_blocking(move || {
        let Ok(meta) = std::fs::metadata(&path) else {
            return String::new();
        };
        let mut truncated = false;
        let read = if meta.len() <= 32_768 {
            std::fs::read(&path)
        } else {
            truncated = true;
            std::fs::OpenOptions::new()
                .read(true)
                .open(&path)
                .and_then(|mut file| {
                    file.seek(SeekFrom::End(-(32_768_i64)))?;
                    let mut buf = Vec::with_capacity(32_768);
                    file.read_to_end(&mut buf)?;
                    Ok(buf)
                })
        };
        let Ok(mut bytes) = read else {
            return String::new();
        };
        if truncated && let Some(pos) = bytes.iter().position(|&b| b == b'\n') {
            bytes.drain(..=pos);
        }
        String::from_utf8_lossy(&bytes).into_owned()
    })
    .await
    .unwrap_or_default()
}

/// Health-checks every service address, dispatching on the address scheme
/// (port of LocalPorts' performHealthCheck: 200-399 = healthy for http(s)).
pub async fn health_all(services: Vec<ServiceConfig>) -> Vec<(String, Health)> {
    cosmic::iced::futures::future::join_all(services.iter().map(check_service)).await
}

/// Health-check mode derived from a pinned address's scheme (ADR 0003).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthMode {
    Http,
    Tcp,
    Off,
}

pub fn health_mode(address: &str) -> HealthMode {
    match crate::config::parse_address(address) {
        Some((_, _, _, scheme)) => match scheme.as_str() {
            "http" | "https" => HealthMode::Http,
            "tcp" => HealthMode::Tcp,
            _ => HealthMode::Off,
        },
        None => HealthMode::Off,
    }
}

async fn check_service(cfg: &ServiceConfig) -> (String, Health) {
    let health = match health_mode(&cfg.address) {
        HealthMode::Http => match crate::config::parse_address(&cfg.address) {
            Some((host, port, path, _)) => match http_status(&host, port, &path).await {
                Ok(status) if (200..=399).contains(&status) => Health::Healthy,
                Ok(status) => Health::Unhealthy(status),
                Err(e) => Health::Failed(e),
            },
            None => Health::Unchecked,
        },
        HealthMode::Tcp => match crate::config::parse_address(&cfg.address) {
            Some((host, port, ..)) => match tcp_probe(&host, port).await {
                Ok(()) => Health::Healthy,
                Err(e) => Health::Failed(e),
            },
            None => Health::Unchecked,
        },
        HealthMode::Off => Health::Unchecked,
    };
    (cfg.id.clone(), health)
}

/// TCP-connect probe for `tcp://` pinned addresses, same 1.5s timeout as HTTP.
async fn tcp_probe(host: &str, port: u16) -> Result<(), String> {
    let timeout = Duration::from_millis(1500);
    let display_host = if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]")
    } else {
        host.to_string()
    };
    let addr = format!("{display_host}:{port}");
    tokio::time::timeout(timeout, tokio::net::TcpStream::connect(&addr))
        .await
        .map_err(|_| "timeout".to_string())?
        .map_err(|e| format!("connect: {e}"))?;
    Ok(())
}

async fn http_status(host: &str, port: u16, path: &str) -> Result<u16, String> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let timeout = Duration::from_millis(1500);
    let display_host = if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]")
    } else {
        host.to_string()
    };
    let addr = format!("{display_host}:{port}");
    let mut stream = tokio::time::timeout(timeout, tokio::net::TcpStream::connect(&addr))
        .await
        .map_err(|_| "timeout".to_string())?
        .map_err(|e| format!("connect: {e}"))?;
    let request = format!("GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n");
    tokio::time::timeout(timeout, stream.write_all(request.as_bytes()))
        .await
        .map_err(|_| "write timeout".to_string())?
        .map_err(|e| e.to_string())?;
    let mut buf = [0u8; 512];
    let n = tokio::time::timeout(timeout, stream.read(&mut buf))
        .await
        .map_err(|_| "read timeout".to_string())?
        .map_err(|e| e.to_string())?;
    let head = String::from_utf8_lossy(&buf[..n]);
    head.split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .ok_or_else(|| "invalid response".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lifecycle_cfg(id: &str) -> ServiceConfig {
        ServiceConfig {
            id: id.to_string(),
            name: "Lifecycle Test".to_string(),
            address: "http://localhost:59999".to_string(),
            start_command: Some("sleep 30".to_string()),
            working_dir: Some("/tmp".to_string()),
            reserved: false,
        }
    }

    #[test]
    fn health_mode_by_scheme() {
        assert_eq!(health_mode("http://localhost:3000"), HealthMode::Http);
        assert_eq!(health_mode("https://localhost:3000"), HealthMode::Http);
        assert_eq!(health_mode("tcp://localhost:4455"), HealthMode::Tcp);
        assert_eq!(health_mode("proc://x:1"), HealthMode::Off);
        assert_eq!(health_mode("garbage"), HealthMode::Off);
        assert_eq!(health_mode("localhost:3000"), HealthMode::Http);
    }

    #[tokio::test]
    async fn spawn_then_stop_updates_registry() {
        let cfg = lifecycle_cfg("test-lifecycle-spawn");
        let pgid = tokio::time::timeout(Duration::from_secs(10), spawn_managed(cfg.clone()))
            .await
            .expect("spawn timed out")
            .expect("spawn failed");
        assert!(pgid > 0);
        assert!(is_managed(&cfg.id));
        tokio::time::timeout(Duration::from_secs(10), stop_managed(cfg.id.clone()))
            .await
            .expect("stop timed out")
            .expect("stop failed");
        for _ in 0..20 {
            if !is_managed(&cfg.id) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        assert!(!is_managed(&cfg.id));
    }

    #[tokio::test]
    async fn restart_stays_managed() {
        let cfg = lifecycle_cfg("test-lifecycle-restart");
        let _ = tokio::time::timeout(Duration::from_secs(10), spawn_managed(cfg.clone()))
            .await
            .expect("spawn timed out")
            .expect("spawn failed");
        assert!(is_managed(&cfg.id));
        let (pgid, stopped_old) =
            tokio::time::timeout(Duration::from_secs(10), restart_managed(cfg.clone()))
                .await
                .expect("restart timed out")
                .expect("restart failed");
        assert!(stopped_old);
        assert!(pgid > 0);
        assert!(is_managed(&cfg.id));
        let _ = tokio::time::timeout(Duration::from_secs(10), stop_managed(cfg.id.clone()))
            .await
            .expect("stop timed out");
    }

    #[tokio::test]
    async fn double_spawn_shares_pgid() {
        let cfg = lifecycle_cfg("test-lifecycle-double");
        let first = tokio::time::timeout(Duration::from_secs(10), spawn_managed(cfg.clone()))
            .await
            .expect("spawn timed out")
            .expect("spawn failed");
        let second = tokio::time::timeout(Duration::from_secs(10), spawn_managed(cfg.clone()))
            .await
            .expect("spawn timed out")
            .expect("spawn failed");
        assert_eq!(first, second);
        assert!(is_managed(&cfg.id));
        let _ = tokio::time::timeout(Duration::from_secs(10), stop_managed(cfg.id.clone()))
            .await
            .expect("stop timed out");
    }

    #[test]
    fn port_reservation_locks_and_releases() {
        let test_port = 58999;
        assert!(!is_port_reserved(test_port));
        assert!(reserve_port(test_port).is_ok());
        assert!(is_port_reserved(test_port));
        // Another bind attempt should fail because Lazy Ports holds it
        assert!(std::net::TcpListener::bind(("127.0.0.1", test_port)).is_err());
        release_port_reservation(test_port);
        assert!(!is_port_reserved(test_port));
        // Now it should be bindable again
        let rebind = std::net::TcpListener::bind(("127.0.0.1", test_port));
        assert!(rebind.is_ok());
    }
}
