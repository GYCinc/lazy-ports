## 20260916T175240 — polling gate, lifecycle serialization, scheme-driven health

- Executed 2026-09-16; ADRs 0001-0003 Accepted.
- Implemented: tick subscription keyed on (popup_open, refresh_secs); per-id op locks (OwnedMutexGuard) + is_managed gates on StartResult AND RestartResult (quality review caught the restart ghost); Health::Unchecked + HealthMode{Http,Tcp,Off} dispatched by address scheme.
- 11 tests green, clippy 0, deployed. Owner visual confirmation pending. Open: OBS address tcp:// switch (owner), single-instance + TLS deferred.
