# ADR 0001: Gate background polling on popup visibility

**Status**: Accepted
**Date**: 2026-09-16

## Context

The tick subscription runs unconditionally: every `refresh_secs` the applet
spawns `lsof` + `ss` and opens a TCP health connection to every pinned service,
whether or not the popup is open or anyone is looking. With the popup closed,
nothing consumes the results — the ports list and health map are only rendered
inside `view_window`.

## Decision

The tick subscription is keyed on `(popup_is_open, refresh_secs)` via
`Subscription::run_with`. Changing either value re-keys the subscription,
tearing down the old interval stream and starting a new one. Closing the popup
stops all polling; opening it starts a fresh stream whose first interval tick
fires immediately, so status is fresh on open.

## Consequences

- Idle applet cost drops to zero (no subprocesses, no TCP connections).
- `states`/`health` can be stale while closed; the immediate first tick on
  open covers this. Process exit events are unaffected (separate event bridge).
- The log-tail read is already driven by `Tick`, so it stops with the popup.
