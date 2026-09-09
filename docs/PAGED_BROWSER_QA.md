# Paged browser completion checks

This describes the isolated synthetic-data smoke test, not a product transport fallback.
The production bounded reader, generated response validation, and error handling are unchanged.

## Why network events alone are insufficient

At commit `92bfe8f`, CI run `34132311660` recorded five requests whose HTTP 200 bodies
reached reader EOF with bytes exactly matching Content-Length. Each token had one CDP
request ending in `loadingFailed(ERR_ABORTED, canceled=true)`, without a source signal
abort or fetch rejection. The macOS diagnostic also observed this after a complete body.

Chromium `151.0.7922.34` has a source-supported counterexample to the assumption that
JavaScript body EOF always implies CDP `loadingFinished`: the delegating consumer can notify
its client before recording its own completed state. Closing the client stream calls consumer
cancellation, which can reenter that still-loading delegate. See Chromium's
[delegating consumer ordering](https://github.com/chromium/chromium/blob/151.0.7922.34/third_party/blink/renderer/platform/loader/fetch/response_body_loader.cc)
and [stream close/cancel path](https://github.com/chromium/chromium/blob/151.0.7922.34/third_party/blink/renderer/core/fetch/body_stream_buffer.cc).
This establishes a possible mechanism, not a captured native stack for every CI event.
Request garbage collection is not the asserted cause.

## Separate classifications

| Observation | Test result |
| --- | --- |
| Exact request intentionally aborted by a filter change | Consume that causal token once |
| Complete validated response followed by the narrowly verified Chromium cancellation | Preserve a `completedResponseCancellations` diagnostic count |
| Any other failed request | Fail the strict unexpected-failure assertion |

The second classification requires **all** of these:

- Exact browser version `151.0.7922.34`; a browser upgrade requires fresh review.
- One nonempty matching token, one failed request, and one CDP `request → aborted` sequence
  with `canceled=true`; no restart, finish, or additional attempt.
- No rejection and no abort of either the source signal or the dispatched request signal.
- HTTP 200, absent/identity content encoding, positive canonical Content-Length within 1 MiB,
  exact consumed byte equality, and reader EOF without error or cancellation.
- Fatal UTF-8 decoding, JSON parsing, and the same generated **response-only** validator used
  by the product succeed. Response kind matches the request, including status request-kind binding.

The observer reads each production chunk once, without cloning, replacing, or canceling the
stream. It temporarily retains at most 1 MiB of chunks and an additional bounded join buffer
for validation, then clears the chunks. Only scalar evidence and validation booleans survive;
response content is not printed or retained in the result. This instrumentation runs only in
the disposable synthetic smoke context, never in the user's Chrome session.

## Regression and release boundary

Classifier regressions reject altered versions, missing/duplicate tokens, wrong status/kind/schema,
compressed or mismatched byte counts, incomplete/error/canceled bodies, signal aborts, fetch
rejections, and extra or different CDP terminal events. A separate production-transport regression
proves that HTTP 200 followed by a body read error still rejects before schema validation.
The smoke also requires every completed query body to validate, exact KPI/pagination behavior,
deletion-cursor invalidation, and zero external or escaped opener requests.

This correction does not certify release readiness. Fresh candidate CI, release-performance
evidence, independent review, and actual Chrome acceptance remain separate gates.
