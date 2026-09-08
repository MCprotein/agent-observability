# Staged ingestion admission

Status: **v1.11 development design; scope approved, reduced admission not enabled.**

**September 8 planning update:** the user approved separating retention target, operational
workspace and filesystem free-space floor. [Storage Budget Policy](STORAGE_BUDGET_POLICY.md)
is the new policy plan, not implemented behavior. The strict-budget investigation below remains
historical evidence and applies to the preserved legacy mode; its VFS path is not resumed.
No unsafe exception, automatic configuration reinterpretation or installed change is authorized.

The user additionally approved evaluating new dependencies and an isolated ingestion process.
[Isolated Ingest](ISOLATED_INGEST.md) defines the proposed enforcement boundary and phased
feasibility gates. This broadens the design scope; it does not enable reduced admission, waive
the memory/rollback gates below, or authorize an unsafe-code policy exception.

## Problem and preserved contract

The configured disk budget remains unchanged. Its default is a planning choice, not a measured
capacity recommendation; see [Configuration](CONFIGURATION.md#저장-공간-예산).
The current `RuntimeControl::collector_admission_diagnostic` subtracts current runtime allocation
when calculating writable headroom, then checks an incremental full store-sized journal proxy,
the maximum batch and active/interrupted report reservations. Current allocation is not charged
twice by that description. This is intentionally conservative. Removing a term without bounding the underlying
write does not establish safe admission.

The ordered transaction in `crates/local-store/src/lib.rs` owns observation/disposition writes,
source cursors, correlation state and projection generation. All must commit or roll back together.
Collector cursor/correlation memory advances only after store success. No schema, privacy,
retention, report reservation or query/cursor contract change is authorized by this design.

## Implementation sequence and gates

1. **Bound the write set before reducing admission.** Local-store owns SQLite geometry and an
   opaque admission plan; runtime owns global disk headroom. The matching mutation guard must
   span preflight through commit. Revalidate SQLite modes and geometry inside `BEGIN IMMEDIATE`.
   Unknown/corrupt schema, unsafe accounting and stale reservations remain fail-closed.
2. **Prove memory safety independently of disk.** The automatic performance protocol requires
   collector RSS p95 at most 96 MiB; peak RSS is currently diagnostic. Neither that percentile
   nor the report reader's 8 MiB SQLite cache target is an ingest-writer memory ceiling. A spill-disabled
   implementation must first specify and enforce a finite dirty-page/temporary-memory bound.
   A sampled low peak alone is not enforcement. Do not enable spill-off solely from a request's
   byte or record count, and do not change a process-global allocator limit inside the shared
   collector as an unreviewed workaround.
3. **Stage, check, then commit.** The existing migration admission module is a reference, not an
   ingest implementation. Any adapted path must account for journal framing, page growth,
   freelist reuse, sparse-page materialization and allocated blocks before commit, then restore
   connection settings on success and failure. A failed final capacity check rolls back the
   whole batch; unsupported but safe modes retain conservative admission, never unsafe success.
   The write-set bound must be enforced before any statement can exceed its admitted journal
   envelope; a final precommit check alone cannot prevent a staging-time overflow.
4. **Prove steady-state usability.** Use a coherent private copy at the installed data scale.
   With both the current and one retired paged snapshot retained, ingest another valid batch and repeat
   publication/ingestion over three generations. Count retained views and current/interrupted
   report reservations throughout. A successful migration or report-only rotation is insufficient.
5. **Integrate only after independent review.** Wire the proven path into collector ordered
   batches. Keep manual imports on conservative admission until their different projection
   workflow has equivalent proof. Re-run exact-head platform and performance gates before
   installed-runtime recovery, release or merge.

## Required adversarial cases

| Case | Required result |
| --- | --- |
| Maximum configured batch, including large correlation state | Bounded memory and disk; atomic commit or typed rollback |
| One late observation for a warm/cold trace | Full-trace rehydration included in the bound, not charged as one row |
| A parent arriving after many children | Topology fan-out included before bulk update |
| Disposition ledger at its pruning boundary | Pruning/index writes included; no silent cursor advance on failure |
| Insufficient final headroom or filesystem space | No observation, cursor, correlation or generation change |
| Process death before/during commit and replay after reopen | Recoverable authority; idempotent retry with no partial batch |
| Sparse files, freelist reuse and supported page sizes | Physical allocation does not exceed the admitted envelope |
| Existing report view plus active/stale reservation | Reservation remains charged; no double lending of headroom |
| Unsupported eligible mode versus corrupt/unsafe state | Conservative fallback for the former, fatal refusal for the latter |

`prepare_archived_trace` in `crates/local-store/src/lifecycle.rs` restores multiple rows using
`INSERT ... SELECT`; `ingest_in_transaction` also resolves every direct child of an arriving
parent. These are concrete reasons why a small input does not imply a small transaction.

## Current evidence boundary

### Private page-geometry experiment — September 8

An isolated coherent copy of the installed v4 authority was compacted with Apple SQLite 3.51.0.
The source was opened read-only for backup and was never opened by `LocalStore` or replaced.
These are logical file sizes, not resource-enforcement bounds:

| Copy | Database bytes | Integrity |
| --- | ---: | --- |
| Original 4096-byte pages | 402,747,392 | Source backup |
| Repacked 4096-byte pages | 395,587,584 | Passed |
| Repacked 1024-byte pages | 287,315,968 | Passed |
| Repacked 2048-byte pages | 313,858,048 | Passed |
| Repacked 8192-byte pages | 310,525,952 | Passed |

For the 1024-byte copy, all eleven tables including `sqlite_sequence` compared equal in both
directions with implicit rowids included. Schema/index SQL compared equal in both directions;
incremental auto-vacuum and DELETE journal mode were preserved. This is observed equality for
this copy, not a general promise that compaction preserves implicit rowids.

The copied 1024-byte authority migrated through the current Rust implementation and published
two retained report views, but the first subsequent new notify was refused under unchanged
collector admission: store allocation 573,317,120 bytes; collector reservation 573,841,408;
remaining writable headroom 364,105,728; deficit 209,735,680; active report reservation zero.
Thus page-size reduction alone does **not** pass steady-state ingestion. This test copies only
the database, not installed JSONL, logs or private detail files, so it understates full-runtime
occupancy. No installed page-size, budget or reservation policy was changed.
The final rerun measured a 209,719,296-byte deficit and independently confirmed unchanged
canonical authority, source cursor, generation/acknowledgement and current view after refusal.
Small physical-allocation variation between runs does not change the rejection result.

A final bounded experiment configured the normal report staging builder for 1024-byte and
2048-byte pages in local test builds only. With the same 1024-byte private authority and two
retained views, the admission deficits were 210,513,920 and 210,735,104 bytes respectively.
Both new writes were refused and both refusal-invariance checks passed. Neither configuration
met the minimum 104,867,840-byte aggregate allocation reduction needed even for the optimistic
database-only copy. The report-page constant was restored to 4096 and the remaining temporary
copies removed. Page-geometry investigation is closed as insufficient, not activated as a fix.

The unresolved implementation gate is future-write accounting against authoritative files with
enforced growth/materialization bounds, or a separately reviewed authority representation.
Do not subtract derived views from admission solely because this experiment identifies their
cost. Whole-transaction atomicity, journal framing, rehydration/fan-out and memory bounds remain
required before collector integration.

The repeatable test-only entrypoint is `private_backup_new_ingest_three_generations` in
`crates/local-collector/src/rotation_diagnostic.rs`. It requires explicit `AO_ROTATION_BACKUP_SOURCE`
and `--ignored --nocapture`, backs up into an owned private runtime, retains two views before
each new write, and requires three committed unique notifications plus matching authoritative
and published counts. Its synthetic regression passes; the installed-scale candidate fails.
Report publication must preserve canonical identity. This is not a replay/idempotence test or
proof of bounded peak RSS, a page-size migration protocol, or installed-runtime acceptance.

### Remaining proof gates

#### Original-page-size admission check — September 8

The existing private-backup new-ingest diagnostic was also run without compaction or a page-size
change. The installed source was read only by SQLite backup; migration and publication operated
on the disposable copy. Before the first new write, with two retained views, it measured:

| Component | Bytes |
| --- | ---: |
| Migrated authority, logical size | 406,126,592 |
| Two retained view files, allocated size | 269,934,592 |
| Store tree, allocated size | 689,463,296 |
| Writable headroom | 247,959,552 |
| Existing collector reservation | 689,987,584 |
| Existing admission deficit | 442,028,032 |
| Authority logical size minus headroom | 158,167,040 |

Even an optimistic reservation equal to just the authority's logical size would not fit.
That comparison is **not a valid journal allowance**: it omits framing, growth and sparse
materialization. It rules out that whole-authority-sized reservation as a recovery for this
copy, not every possible bounded-write algorithm. The copy omits installed non-database files,
so it is still optimistic. The first ingest was refused; canonical authority, cursor,
generation/acknowledgement and current view were unchanged. The owned copy was removed.

The diagnostic now emits these numeric components without source content or identifiers.
No production admission, page size, schema, memory policy or installed runtime changed.
Do not repeat geometry or substitute a whole-authority proxy expecting this deficit to vanish.

The installed v1.10 runtime has not been migrated, replaced, or assigned a larger budget.
Existing v7 migration and report-rotation measurements do not prove this ingestion design.
Reduced reservation must remain disabled until both memory enforcement and post-publication
capacity are demonstrated. Even selective eligibility without a known bulk path remains disabled
until distinct dirty pages, temporary-memory use and peak RSS have enforceable bounds.
The first proof slice adds a private before-commit observer to the existing ordered transaction;
production callers supply a no-op. Test-only observers measure journal bytes and page geometry,
inject failure, and check rollback/retry for a mixed batch and one-input/four-row archived
rehydration. Original invalid-payload and crash/reopen tests remain in place. This measures a
precommit point, not the peak or an enforceable dirty-page/RSS bound; collector admission is unchanged.
Further proof must cover topology fan-out, ledger pruning and steady-state paged snapshots.
Progress and CI status live in the
[v1.11 review checkpoint](reviews/v1.11.0.md).

## Structural-path investigation — disposition pruning

The first prerequisite is implemented in development: `prune_adapter_dispositions` now deletes
through one scalar rowid cutoff instead of materializing the newest 100,000 rowids for `NOT IN`.
It still keeps the largest 100,000 rowids, including gaps and signed extremes; no rowid arithmetic,
schema, transaction boundary, public API, PRAGMA or admission allowance changes are involved.
Other retention/expired-state pruning statements are outside this slice and remain unchanged.

`crates/local-store/src/disposition_pruning_tests.rs` compares the old/new SQL, covers empty and
below/at/above-limit sets, the production-sized bound, reverse insertion order, rollback and
repeated pruning. The exact production SQL's pinned SQLite program must not use `OpenEphemeral`,
`SorterOpen` or `IdxInsert` to build the retained set. This test failed with the old statement
and passed with the scalar cutoff. A fixed 100,001-row synthetic case, deleting one row in either
transaction with the full current schema/indexes, used 1,400,040 VM steps before and 200,044 after. These are instruction counts, not
latency, RSS measurements or a resource-quota proof. The cutoff still scans up to the retained bound.

| Candidate path | What is established | What is not established |
| --- | --- | --- |
| Existing matching disposition replay, without correlation update | The matching branch returns `Duplicate` before any SQL writes in that routine | Entire collector retry has other state/marker behavior; it is not a new-data recovery path |
| New disposition only, no correlation update | Writes disposition ledger and source cursor; with a validated pre-ledger count at most 100,000, each insert can require at most one pruned row | No runtime eligibility check has been enabled; B-tree/index balancing, overflow, freelist/pointer-map and error-path journal/RAM bounds remain open |
| New observation, even when hot | Normalization/reduction and record/topology writes remain in the atomic batch | Rehydration, child fan-out, existing-record decoding and correlated metadata prevent assuming a small write/memory set |
| Oversized legacy ledger or migration | Scalar cutoff preserves the old retained-row semantics | Many rows may still be deleted; no fixed small write allowance follows from removing the retained set |

The 512-byte domain identifier limit is not a bound on the existing database's total content or
on all SQLite internal allocations. Before reducing admission for a new-data path, the remaining
physical write and memory bounds must be proved under the same mutation guard/transaction. Until
then the existing conservative admission remains in force; this optimization alone does not
unblock installed collection or make the isolated-shim candidate viable.
