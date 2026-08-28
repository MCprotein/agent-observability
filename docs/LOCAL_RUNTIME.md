# Local Runtime v1

v0.13.0 standalone runtime is a local-only Rust boundary. It installs private local state,
validates a closed configuration, admits writes against a hard storage budget, and keeps foreground
handoff bounded. It contains no endpoint, email, team identity, envelope, outbox, or network client.
It is a library plus one-shot CLI composition boundary, not a resident server or IPC daemon.

## Install and validate

~~~bash
cargo run -p agent-observability-cli -- init ~/.agent-observability
cargo run -p agent-observability-cli -- config-check ~/.agent-observability/config.json
cargo run -p agent-observability-cli -- runtime-check ~/.agent-observability
cargo run -p agent-observability-cli -- storage-check ~/.agent-observability
cargo run -p agent-observability-cli -- codex-ingest ~/.agent-observability /path/to/private-handoff.jsonl
~~~

init creates the root, logs, queue, state, and runtime directories with mode 0700 and creates
config.json with mode 0600. Existing configuration is validated and preserved. Broad permissions,
symlinks, wrong file types, unsupported schema versions, unknown fields, and values outside policy
bounds fail closed.

The installed configuration is intentionally small:

~~~json
{
  "schema_version": "local_runtime.v1",
  "enabled": true,
  "collection": {
    "file_reconcile_interval_ms": 5000,
    "flush_interval_ms": 5000,
    "max_batch_records": 100,
    "max_batch_bytes": 524288,
    "active_heartbeat_interval_ms": 60000,
    "idle_heartbeat_interval_ms": 300000,
    "local_storage_budget_bytes": 1073741824
  }
}
~~~

## Runtime bounds

- Raw foreground input: at most 1 MiB.
- Privacy-projected local message: at most 64 KiB.
- In-process channel: 64 messages, one normalization worker, nonblocking admission.
- Full, unavailable, and oversized outcomes are bounded counters; they do not wait for drain or
  network.
- One process owns the private runtime directory through an OS-held file lock and random boot
nonce. PID metadata alone never proves ownership.
- Installed-root ingest loads this configuration and holds the singleton through admission, durable
  write, and projection repair. Ingest arguments are runtime roots; direct store-directory config
  bypass is not supported.
- Storage accounting walks allocated filesystem blocks, rejects symlinks, and stops after 4,096
  entries. Admission reserves the worst-case batch against the configured hard cap.
- Pressure transitions are normal -> pressured -> protected -> probe. Two 10-second over-budget
  windows enter pressured; 60 seconds of pressure or 90% disk/queue enters protected; recovery
  requires three 10-second windows below 70%.
- One-shot ingest evaluates disk pressure before writing and rejects protected writes with a bounded
  outcome. The full scheduler state machine and foreground channel are reusable embedding APIs and
  are exercised by the normative subprocess fixture; v0.13 does not install a resident daemon.

v0.13.0 enforces capacity and bounded replay artifacts. Age-based deletion, archive, and export
retention policy remain assigned to v1.2.0.

## Durable state

SQLite local_state.v3 is authoritative. Projection-affecting transactions set
projection_dirty=1; a successful atomic JSONL replacement clears it. A clean reopen does not
rebuild the full projection. Missing or dirty projections are repaired, and stale projection-temp
cleanup is bounded.

## Performance evidence

~~~bash
cargo run -p xtask -- perf local --profile smoke --check
cargo run -p xtask -- perf local --profile release --check
~~~

smoke is non-normative and deletes its temporary output. release uses the protocol in
crates/contracts/performance/local-performance-v1.yaml, writes sanitized evidence under
docs/evidence/local/performance/, and exits nonzero for missing or breached latency, CPU, RSS,
disk, or network evidence. Both profiles delete measured durable stores after validation; release
retains only the sanitized manifest.
