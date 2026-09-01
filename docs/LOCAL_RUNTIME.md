# Local Runtime

v1.0.0 introduced the standalone local-only Rust runtime boundary. v1.2.0 added bounded local
retention and private archive export; v1.4.0 added one-command setup, an isolated built-in demo,
dashboard open, and atomic CLI configuration updates. v1.5.0 adds an explicit, ephemeral loopback
settings UI without adding a resident server, daemon, identity, automatic producer, or external network path.
It installs private local state,
validates a closed configuration, admits writes against a hard storage budget, and keeps foreground
handoff bounded. It contains no external or team endpoint, email, team identity, envelope, outbox, or
network client. The only local endpoint is created by the foreground `ui` command on `127.0.0.1:0`,
uses a private browser session, and expires after inactivity. It is not a resident server or IPC daemon.

## Install and validate

Install a universal macOS binary from GitHub Releases or the authenticated GitHub Package as
documented in the repository README. The package is only a transport for the Rust executable.

~~~bash
agent-observability demo
agent-observability setup
agent-observability ui
agent-observability config show
agent-observability config set retention-days 90
agent-observability dashboard
agent-observability retention-plan ~/.agent-observability
agent-observability retention-apply ~/.agent-observability PLAN_ID /path/to/private-retention-archive.jsonl
agent-observability codex-ingest ~/.agent-observability /path/to/private-handoff.jsonl
agent-observability claude-code-ingest ~/.agent-observability /path/to/private-handoff.jsonl
agent-observability cursor-ingest ~/.agent-observability /path/to/private-handoff.jsonl
agent-observability report ~/.agent-observability /path/to/private-rate-table.json
~~~

`setup` composes private install, store initialization, report generation, and macOS browser open.
`demo` uses an isolated default root and embedded content-free fixture; it never reads an agent log.
`ui` holds only a settings-UI instance singleton, embeds the generated TypeScript application, and
delegates all configuration validation and atomic save behavior back to Rust. A save acquires the
shared runtime singleton only for the mutation. Every supported CLI/UI writer is required to hold the
typed mutation guard. The CLI reads and writes while holding that guard; the UI additionally verifies the
browser revision immediately before the atomic replace. Both release the guard before the next operation.
Direct config file editing is unsupported.
The fragment capability is retained only in same-tab session storage for reload recovery and is removed
after explicit close, an invalid session, or a bootstrap/heartbeat network failure; it is never placed in a
cookie or local storage.
The process stops after 10 minutes without an active browser heartbeat and has an absolute one-hour lifetime.
The lower-level `init` command remains available for automation. Install creates the root, logs,
queue, state, and runtime directories with mode 0700 and creates config.json with mode 0600.
Existing configuration is validated and preserved. Broad permissions,
symlinks, wrong file types, unsupported schema versions, unknown fields, and values outside policy
bounds fail closed.

The installed configuration is intentionally small:

~~~json
{
  "schema_version": "local_runtime.v2",
  "enabled": true,
  "collection": {
    "file_reconcile_interval_ms": 5000,
    "flush_interval_ms": 5000,
    "max_batch_records": 100,
    "max_batch_bytes": 524288,
    "active_heartbeat_interval_ms": 60000,
    "idle_heartbeat_interval_ms": 300000,
    "local_storage_budget_bytes": 1073741824
  },
  "retention": {
    "max_record_age_days": 30,
    "max_archive_records": 10000,
    "max_archive_bytes": 16777216
  }
}
~~~

`config set [root] <option> <value>` acquires the runtime singleton, validates the complete updated
configuration, writes a private temporary file, syncs it, and atomically replaces `config.json`.
Invalid updates leave the previous bytes unchanged. User-facing names, defaults, and bounds are in
[Configuration](CONFIGURATION.md).

## Runtime bounds

- Raw foreground input: at most 1 MiB.
- Privacy-projected local message: at most 64 KiB.
- In-process ingress channel: 64 messages, one normalization writer, nonblocking admission. The
  release fixture uses one CPU execution token and batches at most 32 records or 512 KiB. Including
  one pending 64 KiB message, the durable handoff is bounded to 576 KiB; including the ingress
  channel, the total pipeline payload is bounded to about 4.6 MiB.
- The release fixture applies the same 3 ms driver inter-arrival schedule to baseline and enabled
  supported-rate passes. A separate enabled-only unpaced saturation pass verifies rejection,
  latency, bounded memory, and durable reconciliation. A durability barrier commits every accepted
  supported-rate event before saturation begins. This is workload evidence, not product-side
  pacing or a device-wide CPU guarantee.
- Supported-rate CPU is measured from the first command through barrier completion, including the
  durable commit tail for every accepted event.
- Peak process CPU uses the profile's declared sample interval: one second for normative release
  evidence. This keeps process CPU-time resolution and wall-time normalization aligned.
- The worker creates its drain-evidence marker only after receiving the drain command. It retains the
  marker through drain completion and a parent-confirmed final resource sample, then removes it before
  `drain-complete`. Worker protocol, process exit, local `ps`, sampler result, and monitor shutdown
  waits are bounded; completed sampler and output-reader threads are joined.
- On macOS, each run owns one PTY-backed `nettop` monitor for the worker's full lifetime. Resource
  samples read its latest completed cumulative byte count without spawning another process, while
  the run retains the maximum observed count so traffic remains visible after a socket closes. A
  cycle becomes evidence only when the next header closes it, and each resource read rejects evidence
  older than three seconds. Startup, parsing, unexpected exit, stale evidence, and missing-sample
  failures stop the run. The worker remains alive until a complete cycle that started after durable
  drain completion is observed. Linux instead scans the worker's socket descriptors at each resource
  sample and once after drain; it is point-in-time evidence rather than a continuous byte monitor.
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
  are exercised by the normative subprocess fixture; v1.0 does not install a resident daemon.

Storage admission remains fail-closed at the configured disk budget. Retention is an explicit
operator command rather than an ingest-side implicit delete: pressure never silently overwrites
accepted observations.

## Retention and private archive

`retention-plan` computes a cutoff from the current clock and `max_record_age_days`, then reports a
deterministic bounded selection without writing an archive or changing retention authority. The CLI
still installs and validates the private layout and opens the store, so first open may initialize or
migrate SQLite and a missing or dirty projection may be repaired.
The v1/v2/v3 to v4 migration first bounds any legacy disposition ledger, then enables incremental
auto-vacuum with one atomic full-database rewrite before changing the schema version. Plain store
open refuses a legacy rewrite. Product commands
explicitly admit migration workspace against both the configured storage budget and actual
filesystem availability; less than twice the database file size fails closed before the rewrite.
The supported bounds are 1..3650 days, 1..100,000 archive entries, and 64 KiB..256 MiB per pass.
The default is 30 days, 10,000 entries, and 16 MiB.

Selection is complete-trace only. A trace remains live when any observation is at or after the
cutoff; this prevents an old parent or lifecycle contribution from being removed under a newer
current trace. Unresolved topology remains pinned. A trace with no observation since the cutoff is
eligible even when its last lifecycle state is incomplete, which prevents Codex or Claude Code
sessions without a terminal source signal from remaining forever. If any eligible trace would
exceed the configured pass bounds, the plan stops before it with `truncated=true`; apply rejects the
entire truncated plan so the selected prefix is never partially removed. The operator must raise a
bounded archive limit and create a new plan before applying.

The plan uses a UTC-day cutoff and returns a digest over cutoff, selected trace IDs, event IDs, and
payload hashes. `retention-apply` requires that plan ID and rejects changes to the selected authority
as stale. Unrelated live traces are intentionally outside that digest.

`retention-apply` requires a new archive path outside the managed runtime root. The output parent
must be a private 0700 directory and the archive is created once with mode 0600; existing files,
symlinks, broad paths, and wrong file types fail closed. Each JSONL archive starts with
`agent_observability.retention_archive.v1`, followed only by final sanitized `DurableRecordV1`
entries and a footer with trace/record counts and a SHA-256 record digest. It contains no source
generation, cursor, event ID, payload hash, source prompt/output, or raw opaque ID. A private temp
file is synced and published with a no-overwrite hard link before the archive directory is synced.
Before creating it, the writer holds one stable exclusive lock for the archive parent, performs a
bounded total directory scan, and removes only strict, private crash-left temporary names for that archive while
holding the immediate database transaction. Another store using the same destination fails closed
without removing the active writer's temporary file.
The applied-plan receipt also binds retries to the original archive path and whole-file SHA-256, so
a same-size, internally valid replacement archive is rejected.

The archive is streamed from the indexed SQLite selection, fully written, and synced before SQLite
mutation. One immediate transaction moves privacy-safe canonical span-state hashes to a compact
expiry table, physically deletes matching
observation, source-input, delivery, current-record, and topology rows, and marks the JSONL
projection dirty. Latest source cursors remain, while the content-free disposition ledger is capped
at its newest 100,000 rows. An old cursor
replay fails closed as a cursor conflict; within the newest 100,000 expired span guards, a
semantically identical span at a new cursor stays suppressed and changed state stays a conflict.
Older guards are retired deterministically, and completed retention receipts retain only the newest
1,024 entries. Receipt completion and completed-ledger pruning commit atomically. At most one
uncompacted receipt may exist; a different pass is blocked until that
receipt is recovered. An applied-plan receipt is checked before authority selection and makes post-commit
compaction retryable. Incremental vacuum reclaims at most 16 times the archive byte count in SQLite
pages, and the projection is rebuilt from remaining current records. A failure before SQLite commit can leave
a valid archive while authority remains unchanged; a failure during incremental reclaim leaves already-pruned
authority with a receipt that completes compaction on the same plan ID and archive-path retry. Before-commit failure
must be retried with a new output path because the published archive is never overwritten. A
post-commit retry returns the originally published archive path and does not create another archive.

Archive output belongs outside the managed runtime because files inside it count against the same
disk budget and would defeat reclamation. Moving, retaining, or deleting exported archives is an
operator responsibility; v1.2 does not install an archive scheduler.

Expired records no longer contribute to subsequent local reports. v1.2 does not support correction,
retraction, reinstatement, archive restore, or deterministic aggregate rebuild across retention.
Before any Future TODO feature adds those operations or retained historical aggregates, it must add
a privacy-safe aggregate contribution journal and versioned checkpoints ahead of raw expiry.

## Durable state

SQLite local_state.v4 is authoritative. Projection-affecting transactions set
projection_dirty=1; a successful atomic JSONL replacement clears it. A clean reopen does not
rebuild the full projection. Missing or dirty projections are repaired, and stale projection-temp
cleanup is bounded. Opening v1/v2/v3 authority migrates privacy-projected observation rows to the
v4 trace/span/time index before validation.

## Performance evidence

~~~bash
cargo run -p xtask -- perf local --profile smoke --check
cargo run -p xtask -- perf local --profile release --check
~~~

smoke is non-normative and deletes its temporary output. release uses the protocol in
crates/contracts/performance/local-performance-v1.yaml, writes sanitized evidence under
docs/evidence/local/performance/, and exits nonzero for missing or breached latency, CPU, RSS,
disk, network, or queue-admission evidence. Enabled runs permit at most 1% explicit fail-open
rejections and, after graceful fixture shutdown, must reconcile every enqueued event with one durable
observation. Foreground enqueue does not itself imply durability. Both profiles
delete measured durable stores after validation; release retains only the sanitized manifest. A
release run also requires a clean worktree and records the full source commit SHA. A retained failed
release manifest therefore has to be reviewed and committed before another release-profile attempt;
smoke remains available while the diagnostic commit is being prepared.

## Static report

`report` holds the singleton lock, reads a typed ordered snapshot from SQLite authority, applies the
Rust privacy/cost projector, and writes `logs/agent-observability-report.html` atomically with mode
0600. The optional rate table must satisfy `agent_observability.rate_table.v1`, be at most 1 MiB,
and be a private regular file opened without following symlinks. The generated report is
self-contained and makes no network request when opened with `file://`. The v1.1 UI preserves full
DTO summaries while bounding visible trace, span, and timeline DOM work; saved views persist only
sanitized repo, session, agent, and model dimensions when file-origin storage is available.
