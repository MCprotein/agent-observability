# Local storage lifecycle

Status: **v1.11.0 implementation in progress; not available in the published v1.10.0.**

## Goal

Keep recent observations fast to inspect while bounding local disk use. Hot, warm and cold are
local data lifecycle stages, not Elasticsearch services or a requirement for a remote backend.

| Stage | Intended storage and access | Age since latest trace observation |
| --- | --- | --- |
| Hot | Live observation state and normal report | Less than `hot_days` |
| Warm | Compacted, privacy-safe trace detail; original observation history removed | `hot_days` through `warm_days` |
| Cold | Local archive excluded from the normal report; explicit retrieval | `warm_days` through `delete_after_days` |
| Expired | Managed trace content permanently removed | At least `delete_after_days` |

Thresholds are cumulative ages, not durations added together. Proposed defaults are 7, 30 and
90 days. Physical representation and query evidence must be verified before this feature is
marked complete. Merely labeling unchanged live rows as cold does not meet the performance goal.
Compression is not claimed unless the implemented format and measurements prove it.

## Safety and compatibility

- Existing installations migrate with automatic lifecycle **disabled**. Saving a retention value
  alone must not silently enable automatic deletion.
- The settings screen must explain that enabling lifecycle applies to existing eligible data.
  Reducing a threshold can make previously retained data immediately eligible on the next pass.
- Raw Codex details remain local-only, outside normal reports, sanitized archives and team
  transport. They have an independent retention duration when lifecycle is enabled.
- User-created manual archives outside the managed runtime are never deleted by this policy.
- Eligibility is whole-trace and based on the newest observation. Recent activity and unresolved
  topology prevent partial trace removal. Cursor and replay protections survive expiry within
  the documented bounded replay horizon.
- A lower storage budget does not authorize early deletion of unexpired data. Existing admission
  limits remain fail-closed; cleanup can reclaim eligible data and blocked cleanup must be visible.
- Expiry means removal from managed live/tier storage, not secure erasure of SSD blocks, backups,
  browser downloads or separately exported files.

## Execution and performance acceptance

Automatic maintenance runs only while the optional local collector is running. A standalone
one-shot command must provide the same bounded pass without requiring a daemon. A stopped machine
cannot delete at an exact wall-clock deadline; eligible work resumes on the next maintenance pass.

The worker must use bounded trace/record/byte work, a configurable cadence, nonblocking lock
admission, and a separate blocking executor. It must not scan entire transcripts, render reports
inside the write transaction, perform full-database vacuum, or queue overlapping passes. Oversized
or pinned traces must not cause an unbounded loop. Atomic transitions and restart-safe retries are
required. Disk admission must account for temporary/WAL growth, not only the final compacted size.

## Completion gates

- Versioned configuration migration and Rust/TypeScript validation parity.
- Actual tier movement, explicit cold access, expiry, and bounded replay tests.
- Disabled/default behavior, threshold boundary, late/duplicate input, crash/retry, busy lock,
  oversized trace, storage pressure and source-format parity fixtures.
- Independent review of data-loss, privacy, concurrency and performance risks.
- Settings browser QA and fresh measured maintenance/ingest performance evidence.
- README, ROADMAP, configuration, local runtime, architecture and DESIGN agree with tested behavior.

Until these gates pass, the release remains in progress and no production cleanup is enabled by
development or QA commands.
