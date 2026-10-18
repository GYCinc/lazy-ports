# ADR 0003: Health-check mode derived from the pinned address scheme

**Status**: Accepted
**Date**: 2026-09-16

## Context

Every pinned service is health-checked with a plain HTTP GET regardless of what
it actually is. CBar-imported reality: Hyprwhspr is `systemctl --user start`
(no HTTP), OBS is `flatpak run` (no HTTP on 4455 by default). Both show
permanently "unhealthy", teaching the user to ignore the health dot.

`parse_address` already accepts any scheme and extracts host/port — `tcp://`
addresses parse today. LocalPorts (the inspiration) uses the same convention:
explicit `tcp://` for non-web services.

## Decision

Health mode is derived from the address scheme at check time:

| Scheme         | Check                              | Healthy when        |
| -------------- | ---------------------------------- | ------------------- |
| `http://`      | HTTP GET, 1.5s timeout             | status 200–399      |
| `https://`     | HTTP GET (plaintext limitation, see below) | 200–399     |
| `tcp://`       | TCP connect, 1.5s timeout          | connection succeeds |
| anything else  | no health check                    | n/a (state only)    |

## Consequences

- Non-web services get an honest dot: TCP-open or nothing.
- `https://` still performs a plaintext GET and will read unhealthy for real
  TLS endpoints until TLS support lands (documented limitation, not silently
  wrong — same as today).
- Existing services are unaffected: their `http://` addresses behave identically.
- Stop on a systemd-backed service still only kills the (already exited) shell
  process group; systemd owns the service. Documented limitation, revisit
  trigger: if the owner pins more systemctl-backed services.
