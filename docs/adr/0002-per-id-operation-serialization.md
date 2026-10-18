# ADR 0002: Serialize service start/stop/restart per service id

**Status**: Accepted
**Date**: 2026-09-16

## Context

`spawn_managed` checks the registry, spawns, then inserts — with awaits between
each step. A `StopService` arriving during an in-flight start reads an empty
registry, no-ops, and the service ends up running after the user asked to stop
it. Separately, a fast-failing command can deliver `Exited` before
`StartResult(Ok)`, and the `Ok` arm then overwrites the stopped state — the UI
shows Running for a dead service indefinitely.

## Decision

1. All lifecycle operations (start, stop, restart) acquire a per-id async mutex
   (global `HashMap<String, Arc<tokio::sync::Mutex<()>>>`) for the full
   operation. A stop that arrives during a start waits for the spawn to
   complete, then kills the freshly registered process group.
2. `StartResult(Ok)` confirms the registry still contains the id
   (`services::is_managed`) before setting `Running`; otherwise it leaves the
   state the `Exited` handler wrote.

## Consequences

- Stop-during-start now means "stop the thing that just started" — matches
  user intent.
- Fast-fail commands never show a ghost Running state.
- Lifecycle code must not hold the per-id lock across `Task` boundaries — the
  lock is acquired inside the spawned operation future only.
