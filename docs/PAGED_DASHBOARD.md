# Paged local dashboard

Status: v1.11.0 development implementation; integration and release verification pending.

## Why

The original single-file UI bounds DOM rows but embeds every sanitized span in one HTML artifact. A private
32,317-record diagnostic snapshot passed typed SQLite reading but exceeded the 32 MiB HTML bound.
Adaptive quiet scheduling repairs report-refresh contention; it cannot solve artifact capacity.

The interactive dashboard will read bounded pages without discarding observations. The manual
HTML export stays available within its existing bound. No Elasticsearch, remote collector, login,
automatic deletion, or team dependency is introduced by this change.

## Responsibilities and invariants

| Boundary | Responsibility |
| --- | --- |
| Local store | Authoritative hot/warm records, generation and deletion state; bounded database access |
| Rust application | Shared canonical privacy projection, repository attribution, token and estimated-cost semantics |
| Local query infrastructure | Indexed query representation, resource limits, consistent generation and cursor validation |
| Loopback adapter | Read-only capability-scoped endpoints, bounded requests/responses and concurrency |
| TypeScript | Filters, pagination, selection and availability presentation; no authoritative pricing or aggregation |
| HTML exporter | Existing self-contained export; explicit capacity failure, not silently partial output |

Raw source content is not a query-index or list field. Reuse the existing private-detail endpoint
and policy checks for explicit detail requests. A derived representation is disposable, not an
alternative authority; it must not resurrect retained/deleted or cold-excluded observations.

## Query contract requirements

- Version the wire schema and generate or validate Rust/TypeScript contracts from it.
- Return generation, generated time, exact scope, completeness, total matching counts and bounded
  rows. Preserve unknown/incomplete token and cost states from the existing shared fixtures.
- Allowlist repo/session/agent/model filters and bounded text search; bind parameters, never accept
  SQL, arbitrary file paths or client-defined sort expressions.
- Use stable ordering and opaque, validated cursors bound to generation, filters and query kind.
  Reject stale cursors explicitly; no mixed-generation pages or hidden fallback to offset zero.
- Bound trace pages to 100, span pages to 200, timeline rows to 120 and facet values to 500, matching
  current presentation budgets. List rows omit large detail fields; detail is fetched separately.
- Response-byte, database-work, query-concurrency and memory budgets must be concrete and tested
  before implementation is declared complete. Row limits alone do not bound serialized bytes.
- Overview/filtered aggregates cover all matching hot/warm data, not the current page. Cold archive
  exclusion is visible. A selected trace has an explicitly identified aggregate scope.

## Storage and concurrency design

The current mutable-record store cannot provide a stable multi-request snapshot using a commit
high-water mark alone. Existing generation fences must not be removed, and long read transactions
must not block collection. Choose and review a bounded indexed projection or equivalent snapshot
strategy before wiring UI requests. Do not load a complete `ReportDtoV2` into a server-side cache
as a disguised form of the same capacity problem, or fully reproject the dataset for every page.

Use an immutable private SQLite sidecar, built in a private staging file and atomically published.
Its indexed facts contain sanitized span projections, effective trace repository attribution,
stable sort keys and scalar KPI contributions. Do not add report-specific columns to the authoritative
hot/warm records. Reuse Rust projection semantics; direct SQL over raw durable JSON is not a second
implementation of privacy, pricing or repository propagation.

Construction reads at most 128 records and 2 MiB of serialized records per authority batch, releases
the read transaction, then projects into the sidecar. Oversized single records fail with a typed
capacity state instead of being skipped. A second bounded sidecar pass resolves repository context
and indexes; staging state never becomes query-visible before all passes and fences succeed.

Two counters have distinct jobs:

- `report_generation` fences construction and identifies the source snapshot. New ingest marks
  the published view stale but does not invalidate navigation through that immutable view.
- A new durable `report_visibility_epoch` revokes views when data becomes unavailable or forbidden:
  warm-to-cold movement, expiry, manual retention deletion or future privacy tightening. Hot-to-warm
  movement alone does not revoke the view. Advance it under the existing publication guard before
  destructive work; crash after revocation may leave a pending view, never leaked old data.

Bounded queries take the publication guard through visibility validation and response assembly.
Old-epoch views return `refresh_pending` without rows. Query responses already delivered to a browser
cannot be recalled; subsequent queries and refreshes must not disclose revoked data.

Keep at most the current and one retired immutable snapshot. Cursor leases bind generation,
visibility epoch, normalized filter fingerprint, query kind and last scan key. A newer publication
can retain the prior snapshot for active navigation; another publication or lease expiry explicitly
returns `snapshot_expired`. Restart may expire leases. No keyset cursor may silently become an
offset or switch snapshots.

The current-plus-retired allowance applies only within the current visibility epoch. Advancing
the epoch revokes every old lease and retires all prior-epoch current, retired and staging files
under the destructive-publication boundary. File cleanup failure leaves maintenance pending or
blocked; it must not report deletion complete. Startup removes interrupted/orphaned old-epoch
sidecars before serving queries. This is managed-file retirement, not a forensic erasure guarantee.

Leases retain bounded identifiers/state, not database connections or open file handles between
requests. Each query opens/closes its snapshot within the publication guard. Builders also hold
the publication guard while owning a staging connection, so destructive maintenance cannot claim
cleanup while another process still owns an old-epoch staging file. Ingest still uses short,
separate authority transactions and may invalidate construction via the source-generation fence.

### Shared write admission and compatible index upgrade

A refresh reserves its bounded build/journal allowance plus 401,408 bytes of finalization headroom in one
private durable runtime reservation. Ordinary write admission and migration headroom count the
full active or interrupted reservation in addition to allocated files and filesystem limits.
The reservation contains only a fixed kind, version, random owner nonce and numeric byte ceiling.
Its lifetime lock remains a stable empty inode; metadata is published separately with private
atomic replacement and directory synchronization. After the final failed build retry, cleanup-only
recovery retains scheduler ownership for up to four nonblocking attempts with exponential delays.
Persistent contention or fatal cleanup/task failure is reported through the bounded degraded stage;
the promise remains accounted until a later refresh wake or startup completes guarded recovery.
Cleanup retries do not rebuild the report or spin indefinitely.

Admission and final publication use short, nonblocking runtime mutation guards. Construction
releases that guard so collection can proceed within the remaining budget. Final publication
reloads the current configuration and recounts actual usage; only the validated reservation owner
can exclude its own promised bytes from that final check. Contention leaves the old view intact
and retries. An interrupted reservation stays accounted until guarded staging cleanup succeeds;
process identifiers are not recovery authority.

New sidecars use the private `report_view_staging.v2` layout: the repository, session, turn, agent
and model indexes store their dimension plus source order. Validated v1 sidecars remain readable
through their original bounded query kernel until normal publication replaces them. Metadata and
index shape select the kernel; unknown or inconsistent layouts fail closed. Internal continuation
keys cannot cross kernel versions. The HTTP query schema and opaque cursor contract remain v1.

The separately approved v6→v7 migration isolates acknowledgement in a fixed-width singleton table;
it does not require the existing variable-size metadata table to fit one leaf. Finalization includes
65,536 bytes for catalog publication and 335,872 bytes for the bounded acknowledgement journal,
sparse-page materialization and directory allocation. [Acknowledgement Storage](ACKNOWLEDGEMENT_STORAGE.md)
defines the enforced transaction/layout constraints and migration recovery. This is development
implementation, not installed-runtime or release approval; final integrated evidence remains required.

## Resource budgets and completeness

Initial implementation budgets below require measurement before release, not silent increases:

| Resource | Bound / behavior |
| --- | --- |
| Authority read batch | 128 records and 2 MiB; reject an oversized row explicitly |
| HTTP query request | 8 KiB encoded query; closed allowlisted keys and bounded values |
| Serialized query response | 1 MiB; compact list fields, dedicated bounded detail lookup |
| Query work slice | At most 512 indexed facts and 2 MiB decoded data; cooperative 50 ms deadline |
| Concurrent query work | One worker; overload returns a bounded busy state, no unbounded work queue |
| Active query/cursor leases | At most 16, 120-second idle expiry, 10-minute hard expiry |
| Snapshot files | Current + one retired + one staging file; no accumulating generations |
| SQLite page cache | At most 8 MiB per connection; no full DTO cache |
| Sidecar disk | At most 256 MiB per generation including 8 MiB write/journal reserve; current, retired, staging, publication headroom and durable write reservations count toward the configured local storage budget |

Limits on returned rows are additional to work/byte limits. Sparse filters may produce short or
empty pages with continuation; the UI distinguishes this from the end of results. Text matching
uses the existing allowlisted sanitized fields, not raw content. A single oversized compact row
is a typed error, not an omitted result.

Filtered KPI work runs in bounded slices against the same immutable snapshot. Until the complete
scope is processed, show `pending`, not a page-only total. Exact distinct session/turn counts need
ordered identity passes or a bounded disk-backed strategy; an unbounded in-memory identity set is
not acceptable. Only completed aggregates may be labeled exact. Cache bounded scalar progress and
cursor state; cap active computations with the same lease budget.

Sidecar disk admission includes existing managed data, retired/staging files and journal headroom.
Reject insufficient capacity without deleting authoritative data or enabling lifecycle maintenance.
Private permissions, no-follow file handling, interrupted-build cleanup and pricing/config
fingerprint invalidation are part of the implementation, not optional follow-up hardening.

## Delivery and verification

1. Implement versioned query schemas from this reviewed index design and validate the initial budgets.
2. Implement shared Rust projection and bounded query infrastructure with parity/privacy fixtures.
3. Add the separate loopback shell/query transport and TypeScript pagination without breaking export.
4. Test stale cursors, concurrent ingest/deletion, oversized rows, invalid scope/capability, request
   cancellation, resource pressure and crash recovery. Verify page traversal has no omissions or duplicates.
5. Verify a content-free dataset beyond the single-HTML limit and an isolated copy of local data.
   Check aggregate parity, memory/CPU, responsiveness and existing Chrome-tab QA without OS open calls.
6. Independent code/architecture reviews and fresh final-head CI/performance evidence precede merge
   and release. Prior smaller-scope review and old performance runs do not certify this extension.

Canonical UI behavior: [Design](../DESIGN.md). Existing runtime/export behavior:
[Local Runtime](LOCAL_RUNTIME.md). Lifecycle semantics: [Storage Lifecycle](STORAGE_LIFECYCLE.md).
