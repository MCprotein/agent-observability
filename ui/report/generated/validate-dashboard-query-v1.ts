/* Generated from contracts/dashboard-query-v1.schema.json. Do not edit. */
// @ts-nocheck -- Ajv standalone output is bundled generated JavaScript embedded in TypeScript.
var __getOwnPropNames = Object.getOwnPropertyNames;
var __commonJS = (cb, mod) => function __require() {
  try {
    return mod || (0, cb[__getOwnPropNames(cb)[0]])((mod = { exports: {} }).exports, mod), mod.exports;
  } catch (e) {
    throw mod = 0, e;
  }
};

// node_modules/ajv/dist/runtime/ucs2length.js
var require_ucs2length = __commonJS({
  "node_modules/ajv/dist/runtime/ucs2length.js"(exports) {
    "use strict";
    Object.defineProperty(exports, "__esModule", { value: true });
    function ucs2length(str) {
      const len = str.length;
      let length = 0;
      let pos = 0;
      let value;
      while (pos < len) {
        length++;
        value = str.charCodeAt(pos++);
        if (value >= 55296 && value <= 56319 && pos < len) {
          value = str.charCodeAt(pos);
          if ((value & 64512) === 56320)
            pos++;
        }
      }
      return length;
    }
    exports.default = ucs2length;
    ucs2length.code = 'require("ajv/dist/runtime/ucs2length").default';
  }
});

// validate-dashboard-query-v1.runtime.generated.js
var validateDashboardWireShape = validate20;
var schema31 = { "$schema": "https://json-schema.org/draft/2020-12/schema", "$id": "https://agent-observability.local/contracts/dashboard-query-v1.schema.json", "title": "DashboardQueryWireV1", "description": "Closed request and response wire contract for the bounded standalone paged dashboard. Cursors are opaque server leases and never grant query authority.", "$comment": "Requests are additionally limited to 8192 serialized UTF-8 bytes and responses to 1048576 serialized UTF-8 bytes. These wire limits are enforced separately from row limits.", "x-agent-observability-max-request-serialized-utf8-bytes": 8192, "x-agent-observability-max-response-serialized-utf8-bytes": 1048576, "oneOf": [{ "$ref": "#/$defs/request" }, { "$ref": "#/$defs/response" }], "$defs": { "request": { "title": "DashboardQueryRequestV1", "oneOf": [{ "$ref": "#/$defs/bootstrap_request" }, { "$ref": "#/$defs/traces_request" }, { "$ref": "#/$defs/spans_request" }, { "$ref": "#/$defs/summary_request" }, { "$ref": "#/$defs/span_request" }, { "$ref": "#/$defs/facets_request" }] }, "request_base": { "type": "object", "required": ["schemaVersion"], "properties": { "schemaVersion": { "const": "agent_observability.dashboard_query.v1" }, "filters": { "$ref": "#/$defs/filters" } } }, "bootstrap_request": { "title": "DashboardBootstrapRequestV1", "type": "object", "allOf": [{ "$ref": "#/$defs/request_base" }, { "type": "object", "required": ["kind"], "properties": { "kind": { "const": "bootstrap" } } }], "unevaluatedProperties": false }, "traces_request": { "title": "DashboardTracesRequestV1", "type": "object", "allOf": [{ "$ref": "#/$defs/request_base" }, { "type": "object", "required": ["kind", "snapshotId"], "properties": { "kind": { "const": "traces" }, "snapshotId": { "$ref": "#/$defs/snapshot_id" }, "cursor": { "$ref": "#/$defs/cursor" } } }], "unevaluatedProperties": false }, "spans_request": { "title": "DashboardSpansRequestV1", "type": "object", "allOf": [{ "$ref": "#/$defs/request_base" }, { "type": "object", "required": ["kind", "snapshotId", "traceId"], "properties": { "kind": { "const": "spans" }, "snapshotId": { "$ref": "#/$defs/snapshot_id" }, "traceId": { "$ref": "#/$defs/projected_id" }, "cursor": { "$ref": "#/$defs/cursor" } } }], "unevaluatedProperties": false }, "summary_request": { "title": "DashboardSummaryRequestV1", "type": "object", "allOf": [{ "$ref": "#/$defs/request_base" }, { "type": "object", "required": ["kind", "snapshotId"], "properties": { "kind": { "const": "summary" }, "snapshotId": { "$ref": "#/$defs/snapshot_id" }, "cursor": { "$ref": "#/$defs/cursor" } } }], "unevaluatedProperties": false }, "span_request": { "title": "DashboardSpanRequestV1", "type": "object", "allOf": [{ "$ref": "#/$defs/request_base" }, { "type": "object", "required": ["kind", "snapshotId", "traceId", "spanId"], "properties": { "kind": { "const": "span" }, "snapshotId": { "$ref": "#/$defs/snapshot_id" }, "traceId": { "$ref": "#/$defs/projected_id" }, "spanId": { "$ref": "#/$defs/projected_id" } } }], "unevaluatedProperties": false }, "facets_request": { "title": "DashboardFacetsRequestV1", "type": "object", "allOf": [{ "$ref": "#/$defs/request_base" }, { "type": "object", "required": ["kind", "snapshotId"], "properties": { "kind": { "const": "facets" }, "snapshotId": { "$ref": "#/$defs/snapshot_id" }, "cursor": { "$ref": "#/$defs/cursor" } } }], "unevaluatedProperties": false }, "filters": { "title": "DashboardFiltersV1", "type": "object", "additionalProperties": false, "properties": { "repo": { "$ref": "#/$defs/filter_values" }, "session": { "$ref": "#/$defs/filter_values" }, "agent": { "$ref": "#/$defs/filter_values" }, "model": { "$ref": "#/$defs/filter_values" }, "text": { "type": "string", "minLength": 1, "maxLength": 512 } } }, "filter_values": { "type": "array", "maxItems": 16, "uniqueItems": true, "items": { "type": "string", "minLength": 1, "maxLength": 256 } }, "nullable_cursor": { "oneOf": [{ "$ref": "#/$defs/cursor" }, { "type": "null" }] }, "cursor": { "type": "string", "minLength": 1, "maxLength": 2048, "description": "Opaque server-issued lease token bound to the normalized query, snapshot generation, visibility epoch, query kind and scan key. Clients must not parse or construct it." }, "projected_id": { "type": "string", "pattern": "^id:sha256:[0-9a-f]{64}$" }, "snapshot_id": { "type": "string", "pattern": "^[0-9a-f]{64}$" }, "decimal_u64": { "type": "string", "minLength": 1, "maxLength": 20, "pattern": "^(0|[1-9][0-9]{0,19})$", "description": "Unsigned 64-bit counter serialized as a decimal string to avoid JavaScript precision loss. Semantic validators additionally reject values above u64::MAX." }, "snapshot": { "title": "DashboardSnapshotV1", "type": "object", "additionalProperties": false, "required": ["id", "generation", "visibilityEpoch", "generatedAt", "state"], "properties": { "id": { "$ref": "#/$defs/snapshot_id" }, "generation": { "$ref": "#/$defs/decimal_u64" }, "visibilityEpoch": { "$ref": "#/$defs/decimal_u64" }, "generatedAt": { "type": "string", "minLength": 1, "maxLength": 64 }, "state": { "$ref": "#/$defs/snapshot_state" } } }, "snapshot_state": { "title": "DashboardSnapshotStateV1", "type": "string", "enum": ["current", "stale"] }, "scope": { "title": "DashboardQueryScopeV1", "type": "object", "additionalProperties": false, "required": ["filters", "selectedTraceId", "coldExcluded"], "properties": { "filters": { "$ref": "#/$defs/filters" }, "selectedTraceId": { "oneOf": [{ "$ref": "#/$defs/projected_id" }, { "type": "null" }] }, "coldExcluded": { "const": true } } }, "work": { "title": "DashboardWorkV1", "description": "KPI values have no independent generation field and therefore inherit the enclosing snapshot generation by construction.", "oneOf": [{ "title": "DashboardPendingWorkV1", "type": "object", "additionalProperties": false, "required": ["state"], "properties": { "state": { "const": "pending" } } }, { "title": "DashboardCompleteWorkV1", "type": "object", "additionalProperties": false, "required": ["state", "kpis"], "properties": { "state": { "const": "complete" }, "kpis": { "$ref": "#/$defs/kpis" } } }] }, "kpis": { "title": "DashboardKpisV1", "type": "object", "additionalProperties": false, "required": ["sessions", "turns", "llm", "tools", "errors", "inputTokens", "outputTokens", "totalTokens", "tokenStatus", "estimatedCost", "costStatus", "currency"], "properties": { "sessions": { "$ref": "#/$defs/safe_count" }, "turns": { "$ref": "#/$defs/safe_count" }, "llm": { "$ref": "#/$defs/safe_count" }, "tools": { "$ref": "#/$defs/safe_count" }, "errors": { "$ref": "#/$defs/safe_count" }, "inputTokens": { "$ref": "#/$defs/nullable_safe_count" }, "outputTokens": { "$ref": "#/$defs/nullable_safe_count" }, "totalTokens": { "$ref": "#/$defs/nullable_safe_count" }, "tokenStatus": { "$ref": "#/$defs/token_status" }, "estimatedCost": { "oneOf": [{ "type": "number", "minimum": 0, "maximum": 1e12 }, { "type": "null" }] }, "costStatus": { "$ref": "#/$defs/cost_status" }, "currency": { "oneOf": [{ "type": "string", "minLength": 1, "maxLength": 16 }, { "type": "null" }] } }, "allOf": [{ "oneOf": [{ "properties": { "tokenStatus": { "const": "complete" }, "totalTokens": { "$ref": "#/$defs/safe_count" } } }, { "properties": { "tokenStatus": { "const": "incomplete" }, "totalTokens": { "type": "null" } } }, { "properties": { "tokenStatus": { "const": "unavailable" }, "inputTokens": { "type": "null" }, "outputTokens": { "type": "null" }, "totalTokens": { "type": "null" } } }] }, { "oneOf": [{ "properties": { "costStatus": { "const": "estimated" }, "estimatedCost": { "type": "number", "minimum": 0, "maximum": 1e12 }, "currency": { "type": "string", "minLength": 1, "maxLength": 16 } } }, { "properties": { "costStatus": { "const": "incomplete" } } }, { "properties": { "costStatus": { "const": "unknown" }, "estimatedCost": { "type": "null" }, "currency": { "type": "null" } } }] }] }, "safe_count": { "type": "integer", "minimum": 0, "maximum": 9007199254740991 }, "nullable_safe_count": { "oneOf": [{ "$ref": "#/$defs/safe_count" }, { "type": "null" }] }, "token_status": { "title": "DashboardTokenStatusV1", "type": "string", "enum": ["complete", "incomplete", "unavailable"] }, "cost_status": { "title": "DashboardCostStatusV1", "type": "string", "enum": ["estimated", "incomplete", "unknown"] }, "pagination": { "title": "DashboardPaginationV1", "type": "object", "additionalProperties": false, "required": ["nextCursor"], "properties": { "nextCursor": { "$ref": "#/$defs/nullable_cursor" }, "total": { "$ref": "#/$defs/safe_count" } } }, "availability_reason": { "title": "DashboardAvailabilityReasonV1", "type": "object", "additionalProperties": false, "required": ["field", "state", "reason"], "properties": { "field": { "$ref": "#/$defs/availability_field" }, "state": { "$ref": "report-dto-v2.schema.json#/$defs/field_availability/properties/state" }, "reason": { "$ref": "report-dto-v2.schema.json#/$defs/field_availability/properties/reason" } }, "oneOf": [{ "type": "object", "required": ["state", "reason"], "properties": { "state": { "const": "source_unavailable" }, "reason": { "enum": ["source_not_provided", "not_evaluated", "partial_token_metrics", "historical_codex_source_not_lookup_eligible", "codex_notify_turn_correlation_unavailable", "ambiguous_trace_repository", "legacy_v1_report"] } } }, { "type": "object", "required": ["state", "reason"], "properties": { "state": { "const": "not_applicable" }, "reason": { "enum": ["span_kind_not_model_backed", "span_kind_has_no_latency", "span_kind_has_no_token_usage", "claude_private_lookup_not_supported", "cursor_private_lookup_not_supported", "codex_span_not_notify_derived", "agent_private_lookup_not_supported"] } } }, { "type": "object", "required": ["state", "reason"], "properties": { "state": { "const": "private_lookup" }, "reason": { "const": "local_opt_in_lookup_required" } } }, { "type": "object", "required": ["state", "reason"], "properties": { "state": { "const": "withheld" }, "reason": { "const": "withheld_by_privacy_policy" } } }] }, "availability_reasons": { "type": "array", "maxItems": 9, "items": { "$ref": "#/$defs/availability_reason" }, "allOf": [{ "contains": { "type": "object", "properties": { "field": { "const": "repository" } }, "required": ["field"] }, "minContains": 0, "maxContains": 1 }, { "contains": { "type": "object", "properties": { "field": { "const": "session" } }, "required": ["field"] }, "minContains": 0, "maxContains": 1 }, { "contains": { "type": "object", "properties": { "field": { "const": "turn" } }, "required": ["field"] }, "minContains": 0, "maxContains": 1 }, { "contains": { "type": "object", "properties": { "field": { "const": "model" } }, "required": ["field"] }, "minContains": 0, "maxContains": 1 }, { "contains": { "type": "object", "properties": { "field": { "const": "tokens" } }, "required": ["field"] }, "minContains": 0, "maxContains": 1 }, { "contains": { "type": "object", "properties": { "field": { "const": "latency" } }, "required": ["field"] }, "minContains": 0, "maxContains": 1 }, { "contains": { "type": "object", "properties": { "field": { "const": "source_location" } }, "required": ["field"] }, "minContains": 0, "maxContains": 1 }, { "contains": { "type": "object", "properties": { "field": { "const": "request_content" } }, "required": ["field"] }, "minContains": 0, "maxContains": 1 }, { "contains": { "type": "object", "properties": { "field": { "const": "response_content" } }, "required": ["field"] }, "minContains": 0, "maxContains": 1 }] }, "availability_field": { "title": "DashboardAvailabilityFieldV1", "type": "string", "enum": ["repository", "session", "turn", "model", "tokens", "latency", "source_location", "request_content", "response_content"] }, "trace_row": { "title": "DashboardTraceRowV1", "type": "object", "additionalProperties": false, "required": ["traceId", "repo", "spanCount", "errorCount", "startTimeUnixMs", "endTimeUnixMs", "availabilityReasons"], "properties": { "traceId": { "$ref": "#/$defs/projected_id" }, "repo": { "type": "string", "minLength": 1, "maxLength": 256 }, "spanCount": { "$ref": "#/$defs/safe_count" }, "errorCount": { "$ref": "#/$defs/safe_count" }, "startTimeUnixMs": { "type": "number", "minimum": 0 }, "endTimeUnixMs": { "oneOf": [{ "type": "number", "minimum": 0 }, { "type": "null" }] }, "availabilityReasons": { "$ref": "#/$defs/availability_reasons" } } }, "span_row": { "title": "DashboardSpanRowV1", "description": "Compact projected list row. It intentionally omits attributes, metrics, source content, response content and private raw detail.", "type": "object", "additionalProperties": false, "required": ["traceId", "spanId", "parentSpanId", "kind", "name", "status", "startTimeUnixMs", "endTimeUnixMs", "repo", "availabilityReasons"], "properties": { "traceId": { "$ref": "#/$defs/projected_id" }, "spanId": { "$ref": "#/$defs/projected_id" }, "parentSpanId": { "oneOf": [{ "$ref": "#/$defs/projected_id" }, { "type": "null" }] }, "kind": { "type": "string", "minLength": 1, "maxLength": 64 }, "name": { "type": "string", "minLength": 1, "maxLength": 256 }, "status": { "type": "string", "minLength": 1, "maxLength": 64 }, "startTimeUnixMs": { "type": "number", "minimum": 0 }, "endTimeUnixMs": { "oneOf": [{ "type": "number", "minimum": 0 }, { "type": "null" }] }, "repo": { "type": "string", "minLength": 1, "maxLength": 256 }, "agent": { "type": "string", "minLength": 1, "maxLength": 128 }, "model": { "type": "string", "minLength": 1, "maxLength": 256 }, "sessionId": { "$ref": "#/$defs/projected_id" }, "turnId": { "$ref": "#/$defs/projected_id" }, "toolName": { "type": "string", "minLength": 1, "maxLength": 128 }, "availabilityReasons": { "$ref": "#/$defs/availability_reasons" } } }, "facet_row": { "title": "DashboardFacetRowV1", "type": "object", "additionalProperties": false, "required": ["dimension", "value"], "properties": { "dimension": { "$ref": "#/$defs/facet_dimension" }, "value": { "type": "string", "minLength": 1, "maxLength": 256 } } }, "facet_dimension": { "title": "DashboardFacetDimensionV1", "type": "string", "enum": ["repo", "session", "agent", "model"] }, "limits": { "title": "DashboardQueryLimitsV1", "type": "object", "additionalProperties": false, "required": ["traceRows", "spanRows", "timelineRows", "facetValues", "requestBytes", "responseBytes"], "properties": { "traceRows": { "const": 100 }, "spanRows": { "const": 200 }, "timelineRows": { "const": 120 }, "facetValues": { "const": 500 }, "requestBytes": { "const": 8192 }, "responseBytes": { "const": 1048576 } } }, "response_base": { "type": "object", "required": ["schemaVersion", "snapshot", "scope", "work"], "properties": { "schemaVersion": { "const": "agent_observability.dashboard_query.v1" }, "snapshot": { "$ref": "#/$defs/snapshot" }, "scope": { "$ref": "#/$defs/scope" }, "work": { "$ref": "#/$defs/work" } } }, "bootstrap_response": { "title": "DashboardBootstrapResponseV1", "type": "object", "allOf": [{ "$ref": "#/$defs/response_base" }, { "type": "object", "required": ["kind", "limits"], "properties": { "kind": { "const": "bootstrap" }, "limits": { "$ref": "#/$defs/limits" } } }], "unevaluatedProperties": false }, "traces_response": { "title": "DashboardTracesResponseV1", "type": "object", "allOf": [{ "$ref": "#/$defs/response_base" }, { "type": "object", "required": ["kind", "rows", "pagination"], "properties": { "kind": { "const": "traces" }, "rows": { "type": "array", "maxItems": 100, "items": { "$ref": "#/$defs/trace_row" } }, "pagination": { "$ref": "#/$defs/pagination" } } }], "unevaluatedProperties": false }, "spans_response": { "title": "DashboardSpansResponseV1", "type": "object", "allOf": [{ "$ref": "#/$defs/response_base" }, { "type": "object", "required": ["kind", "rows", "pagination"], "properties": { "kind": { "const": "spans" }, "rows": { "type": "array", "maxItems": 200, "items": { "$ref": "#/$defs/span_row" } }, "pagination": { "$ref": "#/$defs/pagination" } } }], "unevaluatedProperties": false }, "summary_response": { "title": "DashboardSummaryResponseV1", "type": "object", "allOf": [{ "$ref": "#/$defs/response_base" }, { "type": "object", "required": ["kind", "pagination"], "properties": { "kind": { "const": "summary" }, "pagination": { "$ref": "#/$defs/pagination" } } }], "oneOf": [{ "type": "object", "properties": { "work": { "type": "object", "properties": { "state": { "const": "pending" } } }, "pagination": { "type": "object", "properties": { "nextCursor": { "$ref": "#/$defs/cursor" } } } } }, { "type": "object", "properties": { "work": { "type": "object", "properties": { "state": { "const": "complete" } } }, "pagination": { "type": "object", "properties": { "nextCursor": { "type": "null" } } } } }], "unevaluatedProperties": false }, "span_response": { "title": "DashboardSpanResponseV1", "type": "object", "allOf": [{ "$ref": "#/$defs/response_base" }, { "type": "object", "required": ["kind", "detail"], "properties": { "kind": { "const": "span" }, "detail": { "allOf": [{ "$ref": "report-dto-v2.schema.json#/$defs/span" }, { "type": "object", "properties": { "traceId": { "$ref": "#/$defs/projected_id" }, "spanId": { "$ref": "#/$defs/projected_id" }, "parentSpanId": { "oneOf": [{ "$ref": "#/$defs/projected_id" }, { "type": "null" }] } } }] } } }], "unevaluatedProperties": false }, "facets_response": { "title": "DashboardFacetsResponseV1", "type": "object", "allOf": [{ "$ref": "#/$defs/response_base" }, { "type": "object", "required": ["kind", "rows", "pagination"], "properties": { "kind": { "const": "facets" }, "rows": { "type": "array", "maxItems": 500, "items": { "$ref": "#/$defs/facet_row" } }, "pagination": { "$ref": "#/$defs/pagination" } } }], "unevaluatedProperties": false }, "status_response": { "title": "DashboardStatusResponseV1", "type": "object", "additionalProperties": false, "required": ["schemaVersion", "kind", "requestKind", "reason"], "properties": { "schemaVersion": { "const": "agent_observability.dashboard_query.v1" }, "kind": { "const": "status" }, "requestKind": { "$ref": "#/$defs/request_kind" }, "reason": { "$ref": "#/$defs/status_reason" }, "snapshot": { "$ref": "#/$defs/snapshot" } } }, "request_kind": { "title": "DashboardRequestKindV1", "type": "string", "enum": ["bootstrap", "traces", "spans", "summary", "span", "facets"] }, "status_reason": { "title": "DashboardStatusReasonV1", "type": "string", "enum": ["building", "refresh_pending", "snapshot_expired", "busy", "capacity", "invalid_query"] }, "response": { "title": "DashboardQueryResponseV1", "oneOf": [{ "$ref": "#/$defs/bootstrap_response" }, { "$ref": "#/$defs/traces_response" }, { "$ref": "#/$defs/spans_response" }, { "$ref": "#/$defs/summary_response" }, { "$ref": "#/$defs/span_response" }, { "$ref": "#/$defs/facets_response" }, { "$ref": "#/$defs/status_response" }] } } };
var schema32 = { "title": "DashboardQueryRequestV1", "oneOf": [{ "$ref": "#/$defs/bootstrap_request" }, { "$ref": "#/$defs/traces_request" }, { "$ref": "#/$defs/spans_request" }, { "$ref": "#/$defs/summary_request" }, { "$ref": "#/$defs/span_request" }, { "$ref": "#/$defs/facets_request" }] };
var schema33 = { "title": "DashboardBootstrapRequestV1", "type": "object", "allOf": [{ "$ref": "#/$defs/request_base" }, { "type": "object", "required": ["kind"], "properties": { "kind": { "const": "bootstrap" } } }], "unevaluatedProperties": false };
var schema34 = { "type": "object", "required": ["schemaVersion"], "properties": { "schemaVersion": { "const": "agent_observability.dashboard_query.v1" }, "filters": { "$ref": "#/$defs/filters" } } };
var schema35 = { "title": "DashboardFiltersV1", "type": "object", "additionalProperties": false, "properties": { "repo": { "$ref": "#/$defs/filter_values" }, "session": { "$ref": "#/$defs/filter_values" }, "agent": { "$ref": "#/$defs/filter_values" }, "model": { "$ref": "#/$defs/filter_values" }, "text": { "type": "string", "minLength": 1, "maxLength": 512 } } };
var schema36 = { "type": "array", "maxItems": 16, "uniqueItems": true, "items": { "type": "string", "minLength": 1, "maxLength": 256 } };
var func1 = require_ucs2length().default;
function validate24(data, { instancePath = "", parentData, parentDataProperty, rootData = data, dynamicAnchors = {} } = {}) {
  let vErrors = null;
  let errors = 0;
  const evaluated0 = validate24.evaluated;
  if (evaluated0.dynamicProps) {
    evaluated0.props = void 0;
  }
  if (evaluated0.dynamicItems) {
    evaluated0.items = void 0;
  }
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      const _errs1 = errors;
      for (const key0 in data) {
        if (!(key0 === "repo" || key0 === "session" || key0 === "agent" || key0 === "model" || key0 === "text")) {
          validate24.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
          return false;
          break;
        }
      }
      if (_errs1 === errors) {
        if (data.repo !== void 0) {
          let data0 = data.repo;
          const _errs2 = errors;
          const _errs3 = errors;
          if (errors === _errs3) {
            if (Array.isArray(data0)) {
              if (data0.length > 16) {
                validate24.errors = [{ instancePath: instancePath + "/repo", schemaPath: "#/$defs/filter_values/maxItems", keyword: "maxItems", params: { limit: 16 }, message: "must NOT have more than 16 items" }];
                return false;
              } else {
                var valid2 = true;
                const len0 = data0.length;
                for (let i0 = 0; i0 < len0; i0++) {
                  let data1 = data0[i0];
                  const _errs5 = errors;
                  if (errors === _errs5) {
                    if (typeof data1 === "string") {
                      if (func1(data1) > 256) {
                        validate24.errors = [{ instancePath: instancePath + "/repo/" + i0, schemaPath: "#/$defs/filter_values/items/maxLength", keyword: "maxLength", params: { limit: 256 }, message: "must NOT have more than 256 characters" }];
                        return false;
                      } else {
                        if (func1(data1) < 1) {
                          validate24.errors = [{ instancePath: instancePath + "/repo/" + i0, schemaPath: "#/$defs/filter_values/items/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                          return false;
                        }
                      }
                    } else {
                      validate24.errors = [{ instancePath: instancePath + "/repo/" + i0, schemaPath: "#/$defs/filter_values/items/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                      return false;
                    }
                  }
                  var valid2 = _errs5 === errors;
                  if (!valid2) {
                    break;
                  }
                }
                if (valid2) {
                  let i1 = data0.length;
                  let j0;
                  if (i1 > 1) {
                    const indices0 = {};
                    for (; i1--; ) {
                      let item0 = data0[i1];
                      if (typeof item0 !== "string") {
                        continue;
                      }
                      if (typeof indices0[item0] == "number") {
                        j0 = indices0[item0];
                        validate24.errors = [{ instancePath: instancePath + "/repo", schemaPath: "#/$defs/filter_values/uniqueItems", keyword: "uniqueItems", params: { i: i1, j: j0 }, message: "must NOT have duplicate items (items ## " + j0 + " and " + i1 + " are identical)" }];
                        return false;
                        break;
                      }
                      indices0[item0] = i1;
                    }
                  }
                }
              }
            } else {
              validate24.errors = [{ instancePath: instancePath + "/repo", schemaPath: "#/$defs/filter_values/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
              return false;
            }
          }
          var valid0 = _errs2 === errors;
        } else {
          var valid0 = true;
        }
        if (valid0) {
          if (data.session !== void 0) {
            let data2 = data.session;
            const _errs7 = errors;
            const _errs8 = errors;
            if (errors === _errs8) {
              if (Array.isArray(data2)) {
                if (data2.length > 16) {
                  validate24.errors = [{ instancePath: instancePath + "/session", schemaPath: "#/$defs/filter_values/maxItems", keyword: "maxItems", params: { limit: 16 }, message: "must NOT have more than 16 items" }];
                  return false;
                } else {
                  var valid5 = true;
                  const len1 = data2.length;
                  for (let i2 = 0; i2 < len1; i2++) {
                    let data3 = data2[i2];
                    const _errs10 = errors;
                    if (errors === _errs10) {
                      if (typeof data3 === "string") {
                        if (func1(data3) > 256) {
                          validate24.errors = [{ instancePath: instancePath + "/session/" + i2, schemaPath: "#/$defs/filter_values/items/maxLength", keyword: "maxLength", params: { limit: 256 }, message: "must NOT have more than 256 characters" }];
                          return false;
                        } else {
                          if (func1(data3) < 1) {
                            validate24.errors = [{ instancePath: instancePath + "/session/" + i2, schemaPath: "#/$defs/filter_values/items/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                            return false;
                          }
                        }
                      } else {
                        validate24.errors = [{ instancePath: instancePath + "/session/" + i2, schemaPath: "#/$defs/filter_values/items/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                        return false;
                      }
                    }
                    var valid5 = _errs10 === errors;
                    if (!valid5) {
                      break;
                    }
                  }
                  if (valid5) {
                    let i3 = data2.length;
                    let j1;
                    if (i3 > 1) {
                      const indices1 = {};
                      for (; i3--; ) {
                        let item1 = data2[i3];
                        if (typeof item1 !== "string") {
                          continue;
                        }
                        if (typeof indices1[item1] == "number") {
                          j1 = indices1[item1];
                          validate24.errors = [{ instancePath: instancePath + "/session", schemaPath: "#/$defs/filter_values/uniqueItems", keyword: "uniqueItems", params: { i: i3, j: j1 }, message: "must NOT have duplicate items (items ## " + j1 + " and " + i3 + " are identical)" }];
                          return false;
                          break;
                        }
                        indices1[item1] = i3;
                      }
                    }
                  }
                }
              } else {
                validate24.errors = [{ instancePath: instancePath + "/session", schemaPath: "#/$defs/filter_values/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                return false;
              }
            }
            var valid0 = _errs7 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.agent !== void 0) {
              let data4 = data.agent;
              const _errs12 = errors;
              const _errs13 = errors;
              if (errors === _errs13) {
                if (Array.isArray(data4)) {
                  if (data4.length > 16) {
                    validate24.errors = [{ instancePath: instancePath + "/agent", schemaPath: "#/$defs/filter_values/maxItems", keyword: "maxItems", params: { limit: 16 }, message: "must NOT have more than 16 items" }];
                    return false;
                  } else {
                    var valid8 = true;
                    const len2 = data4.length;
                    for (let i4 = 0; i4 < len2; i4++) {
                      let data5 = data4[i4];
                      const _errs15 = errors;
                      if (errors === _errs15) {
                        if (typeof data5 === "string") {
                          if (func1(data5) > 256) {
                            validate24.errors = [{ instancePath: instancePath + "/agent/" + i4, schemaPath: "#/$defs/filter_values/items/maxLength", keyword: "maxLength", params: { limit: 256 }, message: "must NOT have more than 256 characters" }];
                            return false;
                          } else {
                            if (func1(data5) < 1) {
                              validate24.errors = [{ instancePath: instancePath + "/agent/" + i4, schemaPath: "#/$defs/filter_values/items/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                              return false;
                            }
                          }
                        } else {
                          validate24.errors = [{ instancePath: instancePath + "/agent/" + i4, schemaPath: "#/$defs/filter_values/items/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                          return false;
                        }
                      }
                      var valid8 = _errs15 === errors;
                      if (!valid8) {
                        break;
                      }
                    }
                    if (valid8) {
                      let i5 = data4.length;
                      let j2;
                      if (i5 > 1) {
                        const indices2 = {};
                        for (; i5--; ) {
                          let item2 = data4[i5];
                          if (typeof item2 !== "string") {
                            continue;
                          }
                          if (typeof indices2[item2] == "number") {
                            j2 = indices2[item2];
                            validate24.errors = [{ instancePath: instancePath + "/agent", schemaPath: "#/$defs/filter_values/uniqueItems", keyword: "uniqueItems", params: { i: i5, j: j2 }, message: "must NOT have duplicate items (items ## " + j2 + " and " + i5 + " are identical)" }];
                            return false;
                            break;
                          }
                          indices2[item2] = i5;
                        }
                      }
                    }
                  }
                } else {
                  validate24.errors = [{ instancePath: instancePath + "/agent", schemaPath: "#/$defs/filter_values/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                  return false;
                }
              }
              var valid0 = _errs12 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.model !== void 0) {
                let data6 = data.model;
                const _errs17 = errors;
                const _errs18 = errors;
                if (errors === _errs18) {
                  if (Array.isArray(data6)) {
                    if (data6.length > 16) {
                      validate24.errors = [{ instancePath: instancePath + "/model", schemaPath: "#/$defs/filter_values/maxItems", keyword: "maxItems", params: { limit: 16 }, message: "must NOT have more than 16 items" }];
                      return false;
                    } else {
                      var valid11 = true;
                      const len3 = data6.length;
                      for (let i6 = 0; i6 < len3; i6++) {
                        let data7 = data6[i6];
                        const _errs20 = errors;
                        if (errors === _errs20) {
                          if (typeof data7 === "string") {
                            if (func1(data7) > 256) {
                              validate24.errors = [{ instancePath: instancePath + "/model/" + i6, schemaPath: "#/$defs/filter_values/items/maxLength", keyword: "maxLength", params: { limit: 256 }, message: "must NOT have more than 256 characters" }];
                              return false;
                            } else {
                              if (func1(data7) < 1) {
                                validate24.errors = [{ instancePath: instancePath + "/model/" + i6, schemaPath: "#/$defs/filter_values/items/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                                return false;
                              }
                            }
                          } else {
                            validate24.errors = [{ instancePath: instancePath + "/model/" + i6, schemaPath: "#/$defs/filter_values/items/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                            return false;
                          }
                        }
                        var valid11 = _errs20 === errors;
                        if (!valid11) {
                          break;
                        }
                      }
                      if (valid11) {
                        let i7 = data6.length;
                        let j3;
                        if (i7 > 1) {
                          const indices3 = {};
                          for (; i7--; ) {
                            let item3 = data6[i7];
                            if (typeof item3 !== "string") {
                              continue;
                            }
                            if (typeof indices3[item3] == "number") {
                              j3 = indices3[item3];
                              validate24.errors = [{ instancePath: instancePath + "/model", schemaPath: "#/$defs/filter_values/uniqueItems", keyword: "uniqueItems", params: { i: i7, j: j3 }, message: "must NOT have duplicate items (items ## " + j3 + " and " + i7 + " are identical)" }];
                              return false;
                              break;
                            }
                            indices3[item3] = i7;
                          }
                        }
                      }
                    }
                  } else {
                    validate24.errors = [{ instancePath: instancePath + "/model", schemaPath: "#/$defs/filter_values/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                    return false;
                  }
                }
                var valid0 = _errs17 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.text !== void 0) {
                  let data8 = data.text;
                  const _errs22 = errors;
                  if (errors === _errs22) {
                    if (typeof data8 === "string") {
                      if (func1(data8) > 512) {
                        validate24.errors = [{ instancePath: instancePath + "/text", schemaPath: "#/properties/text/maxLength", keyword: "maxLength", params: { limit: 512 }, message: "must NOT have more than 512 characters" }];
                        return false;
                      } else {
                        if (func1(data8) < 1) {
                          validate24.errors = [{ instancePath: instancePath + "/text", schemaPath: "#/properties/text/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                          return false;
                        }
                      }
                    } else {
                      validate24.errors = [{ instancePath: instancePath + "/text", schemaPath: "#/properties/text/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                      return false;
                    }
                  }
                  var valid0 = _errs22 === errors;
                } else {
                  var valid0 = true;
                }
              }
            }
          }
        }
      }
    } else {
      validate24.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate24.errors = vErrors;
  return errors === 0;
}
validate24.evaluated = { "props": true, "dynamicProps": false, "dynamicItems": false };
function validate23(data, { instancePath = "", parentData, parentDataProperty, rootData = data, dynamicAnchors = {} } = {}) {
  let vErrors = null;
  let errors = 0;
  const evaluated0 = validate23.evaluated;
  if (evaluated0.dynamicProps) {
    evaluated0.props = void 0;
  }
  if (evaluated0.dynamicItems) {
    evaluated0.items = void 0;
  }
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.schemaVersion === void 0 && (missing0 = "schemaVersion")) {
        validate23.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        if (data.schemaVersion !== void 0) {
          const _errs1 = errors;
          if ("agent_observability.dashboard_query.v1" !== data.schemaVersion) {
            validate23.errors = [{ instancePath: instancePath + "/schemaVersion", schemaPath: "#/properties/schemaVersion/const", keyword: "const", params: { allowedValue: "agent_observability.dashboard_query.v1" }, message: "must be equal to constant" }];
            return false;
          }
          var valid0 = _errs1 === errors;
        } else {
          var valid0 = true;
        }
        if (valid0) {
          if (data.filters !== void 0) {
            const _errs2 = errors;
            if (!validate24(data.filters, { instancePath: instancePath + "/filters", parentData: data, parentDataProperty: "filters", rootData, dynamicAnchors })) {
              vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
              errors = vErrors.length;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
        }
      }
    } else {
      validate23.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate23.errors = vErrors;
  return errors === 0;
}
validate23.evaluated = { "props": { "schemaVersion": true, "filters": true }, "dynamicProps": false, "dynamicItems": false };
function validate22(data, { instancePath = "", parentData, parentDataProperty, rootData = data, dynamicAnchors = {} } = {}) {
  let vErrors = null;
  let errors = 0;
  const evaluated0 = validate22.evaluated;
  if (evaluated0.dynamicProps) {
    evaluated0.props = void 0;
  }
  if (evaluated0.dynamicItems) {
    evaluated0.items = void 0;
  }
  const _errs1 = errors;
  if (!validate23(data, { instancePath, parentData, parentDataProperty, rootData, dynamicAnchors })) {
    vErrors = vErrors === null ? validate23.errors : vErrors.concat(validate23.errors);
    errors = vErrors.length;
  }
  var valid0 = _errs1 === errors;
  if (valid0) {
    const _errs2 = errors;
    if (errors === _errs2) {
      if (data && typeof data == "object" && !Array.isArray(data)) {
        let missing0;
        if (data.kind === void 0 && (missing0 = "kind")) {
          validate22.errors = [{ instancePath, schemaPath: "#/allOf/1/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
          return false;
        } else {
          if (data.kind !== void 0) {
            if ("bootstrap" !== data.kind) {
              validate22.errors = [{ instancePath: instancePath + "/kind", schemaPath: "#/allOf/1/properties/kind/const", keyword: "const", params: { allowedValue: "bootstrap" }, message: "must be equal to constant" }];
              return false;
            }
          }
        }
      } else {
        validate22.errors = [{ instancePath, schemaPath: "#/allOf/1/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
        return false;
      }
    }
    var valid0 = _errs2 === errors;
  }
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      for (const key0 in data) {
        if (key0 !== "kind" && key0 !== "schemaVersion" && key0 !== "filters") {
          validate22.errors = [{ instancePath, schemaPath: "#/unevaluatedProperties", keyword: "unevaluatedProperties", params: { unevaluatedProperty: key0 }, message: "must NOT have unevaluated properties" }];
          return false;
          break;
        }
      }
    } else {
      validate22.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate22.errors = vErrors;
  return errors === 0;
}
validate22.evaluated = { "props": true, "dynamicProps": false, "dynamicItems": false };
var schema40 = { "title": "DashboardTracesRequestV1", "type": "object", "allOf": [{ "$ref": "#/$defs/request_base" }, { "type": "object", "required": ["kind", "snapshotId"], "properties": { "kind": { "const": "traces" }, "snapshotId": { "$ref": "#/$defs/snapshot_id" }, "cursor": { "$ref": "#/$defs/cursor" } } }], "unevaluatedProperties": false };
var schema41 = { "type": "string", "pattern": "^[0-9a-f]{64}$" };
var schema42 = { "type": "string", "minLength": 1, "maxLength": 2048, "description": "Opaque server-issued lease token bound to the normalized query, snapshot generation, visibility epoch, query kind and scan key. Clients must not parse or construct it." };
var pattern4 = new RegExp("^[0-9a-f]{64}$", "u");
function validate28(data, { instancePath = "", parentData, parentDataProperty, rootData = data, dynamicAnchors = {} } = {}) {
  let vErrors = null;
  let errors = 0;
  const evaluated0 = validate28.evaluated;
  if (evaluated0.dynamicProps) {
    evaluated0.props = void 0;
  }
  if (evaluated0.dynamicItems) {
    evaluated0.items = void 0;
  }
  const _errs1 = errors;
  if (!validate23(data, { instancePath, parentData, parentDataProperty, rootData, dynamicAnchors })) {
    vErrors = vErrors === null ? validate23.errors : vErrors.concat(validate23.errors);
    errors = vErrors.length;
  }
  var valid0 = _errs1 === errors;
  if (valid0) {
    const _errs2 = errors;
    if (errors === _errs2) {
      if (data && typeof data == "object" && !Array.isArray(data)) {
        let missing0;
        if (data.kind === void 0 && (missing0 = "kind") || data.snapshotId === void 0 && (missing0 = "snapshotId")) {
          validate28.errors = [{ instancePath, schemaPath: "#/allOf/1/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
          return false;
        } else {
          if (data.kind !== void 0) {
            const _errs4 = errors;
            if ("traces" !== data.kind) {
              validate28.errors = [{ instancePath: instancePath + "/kind", schemaPath: "#/allOf/1/properties/kind/const", keyword: "const", params: { allowedValue: "traces" }, message: "must be equal to constant" }];
              return false;
            }
            var valid1 = _errs4 === errors;
          } else {
            var valid1 = true;
          }
          if (valid1) {
            if (data.snapshotId !== void 0) {
              let data1 = data.snapshotId;
              const _errs5 = errors;
              const _errs6 = errors;
              if (errors === _errs6) {
                if (typeof data1 === "string") {
                  if (!pattern4.test(data1)) {
                    validate28.errors = [{ instancePath: instancePath + "/snapshotId", schemaPath: "#/$defs/snapshot_id/pattern", keyword: "pattern", params: { pattern: "^[0-9a-f]{64}$" }, message: 'must match pattern "^[0-9a-f]{64}$"' }];
                    return false;
                  }
                } else {
                  validate28.errors = [{ instancePath: instancePath + "/snapshotId", schemaPath: "#/$defs/snapshot_id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                  return false;
                }
              }
              var valid1 = _errs5 === errors;
            } else {
              var valid1 = true;
            }
            if (valid1) {
              if (data.cursor !== void 0) {
                let data2 = data.cursor;
                const _errs8 = errors;
                const _errs9 = errors;
                if (errors === _errs9) {
                  if (typeof data2 === "string") {
                    if (func1(data2) > 2048) {
                      validate28.errors = [{ instancePath: instancePath + "/cursor", schemaPath: "#/$defs/cursor/maxLength", keyword: "maxLength", params: { limit: 2048 }, message: "must NOT have more than 2048 characters" }];
                      return false;
                    } else {
                      if (func1(data2) < 1) {
                        validate28.errors = [{ instancePath: instancePath + "/cursor", schemaPath: "#/$defs/cursor/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                        return false;
                      }
                    }
                  } else {
                    validate28.errors = [{ instancePath: instancePath + "/cursor", schemaPath: "#/$defs/cursor/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                    return false;
                  }
                }
                var valid1 = _errs8 === errors;
              } else {
                var valid1 = true;
              }
            }
          }
        }
      } else {
        validate28.errors = [{ instancePath, schemaPath: "#/allOf/1/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
        return false;
      }
    }
    var valid0 = _errs2 === errors;
  }
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      for (const key0 in data) {
        if (key0 !== "kind" && key0 !== "snapshotId" && key0 !== "cursor" && key0 !== "schemaVersion" && key0 !== "filters") {
          validate28.errors = [{ instancePath, schemaPath: "#/unevaluatedProperties", keyword: "unevaluatedProperties", params: { unevaluatedProperty: key0 }, message: "must NOT have unevaluated properties" }];
          return false;
          break;
        }
      }
    } else {
      validate28.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate28.errors = vErrors;
  return errors === 0;
}
validate28.evaluated = { "props": true, "dynamicProps": false, "dynamicItems": false };
var schema43 = { "title": "DashboardSpansRequestV1", "type": "object", "allOf": [{ "$ref": "#/$defs/request_base" }, { "type": "object", "required": ["kind", "snapshotId", "traceId"], "properties": { "kind": { "const": "spans" }, "snapshotId": { "$ref": "#/$defs/snapshot_id" }, "traceId": { "$ref": "#/$defs/projected_id" }, "cursor": { "$ref": "#/$defs/cursor" } } }], "unevaluatedProperties": false };
var schema45 = { "type": "string", "pattern": "^id:sha256:[0-9a-f]{64}$" };
var pattern6 = new RegExp("^id:sha256:[0-9a-f]{64}$", "u");
function validate31(data, { instancePath = "", parentData, parentDataProperty, rootData = data, dynamicAnchors = {} } = {}) {
  let vErrors = null;
  let errors = 0;
  const evaluated0 = validate31.evaluated;
  if (evaluated0.dynamicProps) {
    evaluated0.props = void 0;
  }
  if (evaluated0.dynamicItems) {
    evaluated0.items = void 0;
  }
  const _errs1 = errors;
  if (!validate23(data, { instancePath, parentData, parentDataProperty, rootData, dynamicAnchors })) {
    vErrors = vErrors === null ? validate23.errors : vErrors.concat(validate23.errors);
    errors = vErrors.length;
  }
  var valid0 = _errs1 === errors;
  if (valid0) {
    const _errs2 = errors;
    if (errors === _errs2) {
      if (data && typeof data == "object" && !Array.isArray(data)) {
        let missing0;
        if (data.kind === void 0 && (missing0 = "kind") || data.snapshotId === void 0 && (missing0 = "snapshotId") || data.traceId === void 0 && (missing0 = "traceId")) {
          validate31.errors = [{ instancePath, schemaPath: "#/allOf/1/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
          return false;
        } else {
          if (data.kind !== void 0) {
            const _errs4 = errors;
            if ("spans" !== data.kind) {
              validate31.errors = [{ instancePath: instancePath + "/kind", schemaPath: "#/allOf/1/properties/kind/const", keyword: "const", params: { allowedValue: "spans" }, message: "must be equal to constant" }];
              return false;
            }
            var valid1 = _errs4 === errors;
          } else {
            var valid1 = true;
          }
          if (valid1) {
            if (data.snapshotId !== void 0) {
              let data1 = data.snapshotId;
              const _errs5 = errors;
              const _errs6 = errors;
              if (errors === _errs6) {
                if (typeof data1 === "string") {
                  if (!pattern4.test(data1)) {
                    validate31.errors = [{ instancePath: instancePath + "/snapshotId", schemaPath: "#/$defs/snapshot_id/pattern", keyword: "pattern", params: { pattern: "^[0-9a-f]{64}$" }, message: 'must match pattern "^[0-9a-f]{64}$"' }];
                    return false;
                  }
                } else {
                  validate31.errors = [{ instancePath: instancePath + "/snapshotId", schemaPath: "#/$defs/snapshot_id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                  return false;
                }
              }
              var valid1 = _errs5 === errors;
            } else {
              var valid1 = true;
            }
            if (valid1) {
              if (data.traceId !== void 0) {
                let data2 = data.traceId;
                const _errs8 = errors;
                const _errs9 = errors;
                if (errors === _errs9) {
                  if (typeof data2 === "string") {
                    if (!pattern6.test(data2)) {
                      validate31.errors = [{ instancePath: instancePath + "/traceId", schemaPath: "#/$defs/projected_id/pattern", keyword: "pattern", params: { pattern: "^id:sha256:[0-9a-f]{64}$" }, message: 'must match pattern "^id:sha256:[0-9a-f]{64}$"' }];
                      return false;
                    }
                  } else {
                    validate31.errors = [{ instancePath: instancePath + "/traceId", schemaPath: "#/$defs/projected_id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                    return false;
                  }
                }
                var valid1 = _errs8 === errors;
              } else {
                var valid1 = true;
              }
              if (valid1) {
                if (data.cursor !== void 0) {
                  let data3 = data.cursor;
                  const _errs11 = errors;
                  const _errs12 = errors;
                  if (errors === _errs12) {
                    if (typeof data3 === "string") {
                      if (func1(data3) > 2048) {
                        validate31.errors = [{ instancePath: instancePath + "/cursor", schemaPath: "#/$defs/cursor/maxLength", keyword: "maxLength", params: { limit: 2048 }, message: "must NOT have more than 2048 characters" }];
                        return false;
                      } else {
                        if (func1(data3) < 1) {
                          validate31.errors = [{ instancePath: instancePath + "/cursor", schemaPath: "#/$defs/cursor/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                          return false;
                        }
                      }
                    } else {
                      validate31.errors = [{ instancePath: instancePath + "/cursor", schemaPath: "#/$defs/cursor/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                      return false;
                    }
                  }
                  var valid1 = _errs11 === errors;
                } else {
                  var valid1 = true;
                }
              }
            }
          }
        }
      } else {
        validate31.errors = [{ instancePath, schemaPath: "#/allOf/1/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
        return false;
      }
    }
    var valid0 = _errs2 === errors;
  }
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      for (const key0 in data) {
        if (key0 !== "kind" && key0 !== "snapshotId" && key0 !== "traceId" && key0 !== "cursor" && key0 !== "schemaVersion" && key0 !== "filters") {
          validate31.errors = [{ instancePath, schemaPath: "#/unevaluatedProperties", keyword: "unevaluatedProperties", params: { unevaluatedProperty: key0 }, message: "must NOT have unevaluated properties" }];
          return false;
          break;
        }
      }
    } else {
      validate31.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate31.errors = vErrors;
  return errors === 0;
}
validate31.evaluated = { "props": true, "dynamicProps": false, "dynamicItems": false };
var schema47 = { "title": "DashboardSummaryRequestV1", "type": "object", "allOf": [{ "$ref": "#/$defs/request_base" }, { "type": "object", "required": ["kind", "snapshotId"], "properties": { "kind": { "const": "summary" }, "snapshotId": { "$ref": "#/$defs/snapshot_id" }, "cursor": { "$ref": "#/$defs/cursor" } } }], "unevaluatedProperties": false };
function validate34(data, { instancePath = "", parentData, parentDataProperty, rootData = data, dynamicAnchors = {} } = {}) {
  let vErrors = null;
  let errors = 0;
  const evaluated0 = validate34.evaluated;
  if (evaluated0.dynamicProps) {
    evaluated0.props = void 0;
  }
  if (evaluated0.dynamicItems) {
    evaluated0.items = void 0;
  }
  const _errs1 = errors;
  if (!validate23(data, { instancePath, parentData, parentDataProperty, rootData, dynamicAnchors })) {
    vErrors = vErrors === null ? validate23.errors : vErrors.concat(validate23.errors);
    errors = vErrors.length;
  }
  var valid0 = _errs1 === errors;
  if (valid0) {
    const _errs2 = errors;
    if (errors === _errs2) {
      if (data && typeof data == "object" && !Array.isArray(data)) {
        let missing0;
        if (data.kind === void 0 && (missing0 = "kind") || data.snapshotId === void 0 && (missing0 = "snapshotId")) {
          validate34.errors = [{ instancePath, schemaPath: "#/allOf/1/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
          return false;
        } else {
          if (data.kind !== void 0) {
            const _errs4 = errors;
            if ("summary" !== data.kind) {
              validate34.errors = [{ instancePath: instancePath + "/kind", schemaPath: "#/allOf/1/properties/kind/const", keyword: "const", params: { allowedValue: "summary" }, message: "must be equal to constant" }];
              return false;
            }
            var valid1 = _errs4 === errors;
          } else {
            var valid1 = true;
          }
          if (valid1) {
            if (data.snapshotId !== void 0) {
              let data1 = data.snapshotId;
              const _errs5 = errors;
              const _errs6 = errors;
              if (errors === _errs6) {
                if (typeof data1 === "string") {
                  if (!pattern4.test(data1)) {
                    validate34.errors = [{ instancePath: instancePath + "/snapshotId", schemaPath: "#/$defs/snapshot_id/pattern", keyword: "pattern", params: { pattern: "^[0-9a-f]{64}$" }, message: 'must match pattern "^[0-9a-f]{64}$"' }];
                    return false;
                  }
                } else {
                  validate34.errors = [{ instancePath: instancePath + "/snapshotId", schemaPath: "#/$defs/snapshot_id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                  return false;
                }
              }
              var valid1 = _errs5 === errors;
            } else {
              var valid1 = true;
            }
            if (valid1) {
              if (data.cursor !== void 0) {
                let data2 = data.cursor;
                const _errs8 = errors;
                const _errs9 = errors;
                if (errors === _errs9) {
                  if (typeof data2 === "string") {
                    if (func1(data2) > 2048) {
                      validate34.errors = [{ instancePath: instancePath + "/cursor", schemaPath: "#/$defs/cursor/maxLength", keyword: "maxLength", params: { limit: 2048 }, message: "must NOT have more than 2048 characters" }];
                      return false;
                    } else {
                      if (func1(data2) < 1) {
                        validate34.errors = [{ instancePath: instancePath + "/cursor", schemaPath: "#/$defs/cursor/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                        return false;
                      }
                    }
                  } else {
                    validate34.errors = [{ instancePath: instancePath + "/cursor", schemaPath: "#/$defs/cursor/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                    return false;
                  }
                }
                var valid1 = _errs8 === errors;
              } else {
                var valid1 = true;
              }
            }
          }
        }
      } else {
        validate34.errors = [{ instancePath, schemaPath: "#/allOf/1/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
        return false;
      }
    }
    var valid0 = _errs2 === errors;
  }
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      for (const key0 in data) {
        if (key0 !== "kind" && key0 !== "snapshotId" && key0 !== "cursor" && key0 !== "schemaVersion" && key0 !== "filters") {
          validate34.errors = [{ instancePath, schemaPath: "#/unevaluatedProperties", keyword: "unevaluatedProperties", params: { unevaluatedProperty: key0 }, message: "must NOT have unevaluated properties" }];
          return false;
          break;
        }
      }
    } else {
      validate34.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate34.errors = vErrors;
  return errors === 0;
}
validate34.evaluated = { "props": true, "dynamicProps": false, "dynamicItems": false };
var schema50 = { "title": "DashboardSpanRequestV1", "type": "object", "allOf": [{ "$ref": "#/$defs/request_base" }, { "type": "object", "required": ["kind", "snapshotId", "traceId", "spanId"], "properties": { "kind": { "const": "span" }, "snapshotId": { "$ref": "#/$defs/snapshot_id" }, "traceId": { "$ref": "#/$defs/projected_id" }, "spanId": { "$ref": "#/$defs/projected_id" } } }], "unevaluatedProperties": false };
function validate37(data, { instancePath = "", parentData, parentDataProperty, rootData = data, dynamicAnchors = {} } = {}) {
  let vErrors = null;
  let errors = 0;
  const evaluated0 = validate37.evaluated;
  if (evaluated0.dynamicProps) {
    evaluated0.props = void 0;
  }
  if (evaluated0.dynamicItems) {
    evaluated0.items = void 0;
  }
  const _errs1 = errors;
  if (!validate23(data, { instancePath, parentData, parentDataProperty, rootData, dynamicAnchors })) {
    vErrors = vErrors === null ? validate23.errors : vErrors.concat(validate23.errors);
    errors = vErrors.length;
  }
  var valid0 = _errs1 === errors;
  if (valid0) {
    const _errs2 = errors;
    if (errors === _errs2) {
      if (data && typeof data == "object" && !Array.isArray(data)) {
        let missing0;
        if (data.kind === void 0 && (missing0 = "kind") || data.snapshotId === void 0 && (missing0 = "snapshotId") || data.traceId === void 0 && (missing0 = "traceId") || data.spanId === void 0 && (missing0 = "spanId")) {
          validate37.errors = [{ instancePath, schemaPath: "#/allOf/1/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
          return false;
        } else {
          if (data.kind !== void 0) {
            const _errs4 = errors;
            if ("span" !== data.kind) {
              validate37.errors = [{ instancePath: instancePath + "/kind", schemaPath: "#/allOf/1/properties/kind/const", keyword: "const", params: { allowedValue: "span" }, message: "must be equal to constant" }];
              return false;
            }
            var valid1 = _errs4 === errors;
          } else {
            var valid1 = true;
          }
          if (valid1) {
            if (data.snapshotId !== void 0) {
              let data1 = data.snapshotId;
              const _errs5 = errors;
              const _errs6 = errors;
              if (errors === _errs6) {
                if (typeof data1 === "string") {
                  if (!pattern4.test(data1)) {
                    validate37.errors = [{ instancePath: instancePath + "/snapshotId", schemaPath: "#/$defs/snapshot_id/pattern", keyword: "pattern", params: { pattern: "^[0-9a-f]{64}$" }, message: 'must match pattern "^[0-9a-f]{64}$"' }];
                    return false;
                  }
                } else {
                  validate37.errors = [{ instancePath: instancePath + "/snapshotId", schemaPath: "#/$defs/snapshot_id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                  return false;
                }
              }
              var valid1 = _errs5 === errors;
            } else {
              var valid1 = true;
            }
            if (valid1) {
              if (data.traceId !== void 0) {
                let data2 = data.traceId;
                const _errs8 = errors;
                const _errs9 = errors;
                if (errors === _errs9) {
                  if (typeof data2 === "string") {
                    if (!pattern6.test(data2)) {
                      validate37.errors = [{ instancePath: instancePath + "/traceId", schemaPath: "#/$defs/projected_id/pattern", keyword: "pattern", params: { pattern: "^id:sha256:[0-9a-f]{64}$" }, message: 'must match pattern "^id:sha256:[0-9a-f]{64}$"' }];
                      return false;
                    }
                  } else {
                    validate37.errors = [{ instancePath: instancePath + "/traceId", schemaPath: "#/$defs/projected_id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                    return false;
                  }
                }
                var valid1 = _errs8 === errors;
              } else {
                var valid1 = true;
              }
              if (valid1) {
                if (data.spanId !== void 0) {
                  let data3 = data.spanId;
                  const _errs11 = errors;
                  const _errs12 = errors;
                  if (errors === _errs12) {
                    if (typeof data3 === "string") {
                      if (!pattern6.test(data3)) {
                        validate37.errors = [{ instancePath: instancePath + "/spanId", schemaPath: "#/$defs/projected_id/pattern", keyword: "pattern", params: { pattern: "^id:sha256:[0-9a-f]{64}$" }, message: 'must match pattern "^id:sha256:[0-9a-f]{64}$"' }];
                        return false;
                      }
                    } else {
                      validate37.errors = [{ instancePath: instancePath + "/spanId", schemaPath: "#/$defs/projected_id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                      return false;
                    }
                  }
                  var valid1 = _errs11 === errors;
                } else {
                  var valid1 = true;
                }
              }
            }
          }
        }
      } else {
        validate37.errors = [{ instancePath, schemaPath: "#/allOf/1/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
        return false;
      }
    }
    var valid0 = _errs2 === errors;
  }
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      for (const key0 in data) {
        if (key0 !== "kind" && key0 !== "snapshotId" && key0 !== "traceId" && key0 !== "spanId" && key0 !== "schemaVersion" && key0 !== "filters") {
          validate37.errors = [{ instancePath, schemaPath: "#/unevaluatedProperties", keyword: "unevaluatedProperties", params: { unevaluatedProperty: key0 }, message: "must NOT have unevaluated properties" }];
          return false;
          break;
        }
      }
    } else {
      validate37.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate37.errors = vErrors;
  return errors === 0;
}
validate37.evaluated = { "props": true, "dynamicProps": false, "dynamicItems": false };
var schema54 = { "title": "DashboardFacetsRequestV1", "type": "object", "allOf": [{ "$ref": "#/$defs/request_base" }, { "type": "object", "required": ["kind", "snapshotId"], "properties": { "kind": { "const": "facets" }, "snapshotId": { "$ref": "#/$defs/snapshot_id" }, "cursor": { "$ref": "#/$defs/cursor" } } }], "unevaluatedProperties": false };
function validate40(data, { instancePath = "", parentData, parentDataProperty, rootData = data, dynamicAnchors = {} } = {}) {
  let vErrors = null;
  let errors = 0;
  const evaluated0 = validate40.evaluated;
  if (evaluated0.dynamicProps) {
    evaluated0.props = void 0;
  }
  if (evaluated0.dynamicItems) {
    evaluated0.items = void 0;
  }
  const _errs1 = errors;
  if (!validate23(data, { instancePath, parentData, parentDataProperty, rootData, dynamicAnchors })) {
    vErrors = vErrors === null ? validate23.errors : vErrors.concat(validate23.errors);
    errors = vErrors.length;
  }
  var valid0 = _errs1 === errors;
  if (valid0) {
    const _errs2 = errors;
    if (errors === _errs2) {
      if (data && typeof data == "object" && !Array.isArray(data)) {
        let missing0;
        if (data.kind === void 0 && (missing0 = "kind") || data.snapshotId === void 0 && (missing0 = "snapshotId")) {
          validate40.errors = [{ instancePath, schemaPath: "#/allOf/1/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
          return false;
        } else {
          if (data.kind !== void 0) {
            const _errs4 = errors;
            if ("facets" !== data.kind) {
              validate40.errors = [{ instancePath: instancePath + "/kind", schemaPath: "#/allOf/1/properties/kind/const", keyword: "const", params: { allowedValue: "facets" }, message: "must be equal to constant" }];
              return false;
            }
            var valid1 = _errs4 === errors;
          } else {
            var valid1 = true;
          }
          if (valid1) {
            if (data.snapshotId !== void 0) {
              let data1 = data.snapshotId;
              const _errs5 = errors;
              const _errs6 = errors;
              if (errors === _errs6) {
                if (typeof data1 === "string") {
                  if (!pattern4.test(data1)) {
                    validate40.errors = [{ instancePath: instancePath + "/snapshotId", schemaPath: "#/$defs/snapshot_id/pattern", keyword: "pattern", params: { pattern: "^[0-9a-f]{64}$" }, message: 'must match pattern "^[0-9a-f]{64}$"' }];
                    return false;
                  }
                } else {
                  validate40.errors = [{ instancePath: instancePath + "/snapshotId", schemaPath: "#/$defs/snapshot_id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                  return false;
                }
              }
              var valid1 = _errs5 === errors;
            } else {
              var valid1 = true;
            }
            if (valid1) {
              if (data.cursor !== void 0) {
                let data2 = data.cursor;
                const _errs8 = errors;
                const _errs9 = errors;
                if (errors === _errs9) {
                  if (typeof data2 === "string") {
                    if (func1(data2) > 2048) {
                      validate40.errors = [{ instancePath: instancePath + "/cursor", schemaPath: "#/$defs/cursor/maxLength", keyword: "maxLength", params: { limit: 2048 }, message: "must NOT have more than 2048 characters" }];
                      return false;
                    } else {
                      if (func1(data2) < 1) {
                        validate40.errors = [{ instancePath: instancePath + "/cursor", schemaPath: "#/$defs/cursor/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                        return false;
                      }
                    }
                  } else {
                    validate40.errors = [{ instancePath: instancePath + "/cursor", schemaPath: "#/$defs/cursor/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                    return false;
                  }
                }
                var valid1 = _errs8 === errors;
              } else {
                var valid1 = true;
              }
            }
          }
        }
      } else {
        validate40.errors = [{ instancePath, schemaPath: "#/allOf/1/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
        return false;
      }
    }
    var valid0 = _errs2 === errors;
  }
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      for (const key0 in data) {
        if (key0 !== "kind" && key0 !== "snapshotId" && key0 !== "cursor" && key0 !== "schemaVersion" && key0 !== "filters") {
          validate40.errors = [{ instancePath, schemaPath: "#/unevaluatedProperties", keyword: "unevaluatedProperties", params: { unevaluatedProperty: key0 }, message: "must NOT have unevaluated properties" }];
          return false;
          break;
        }
      }
    } else {
      validate40.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate40.errors = vErrors;
  return errors === 0;
}
validate40.evaluated = { "props": true, "dynamicProps": false, "dynamicItems": false };
function validate21(data, { instancePath = "", parentData, parentDataProperty, rootData = data, dynamicAnchors = {} } = {}) {
  let vErrors = null;
  let errors = 0;
  const evaluated0 = validate21.evaluated;
  if (evaluated0.dynamicProps) {
    evaluated0.props = void 0;
  }
  if (evaluated0.dynamicItems) {
    evaluated0.items = void 0;
  }
  const _errs0 = errors;
  let valid0 = false;
  let passing0 = null;
  const _errs1 = errors;
  if (!validate22(data, { instancePath, parentData, parentDataProperty, rootData, dynamicAnchors })) {
    vErrors = vErrors === null ? validate22.errors : vErrors.concat(validate22.errors);
    errors = vErrors.length;
  }
  var _valid0 = _errs1 === errors;
  if (_valid0) {
    valid0 = true;
    passing0 = 0;
    var props0 = true;
  }
  const _errs2 = errors;
  if (!validate28(data, { instancePath, parentData, parentDataProperty, rootData, dynamicAnchors })) {
    vErrors = vErrors === null ? validate28.errors : vErrors.concat(validate28.errors);
    errors = vErrors.length;
  }
  var _valid0 = _errs2 === errors;
  if (_valid0 && valid0) {
    valid0 = false;
    passing0 = [passing0, 1];
  } else {
    if (_valid0) {
      valid0 = true;
      passing0 = 1;
      if (props0 !== true) {
        props0 = true;
      }
    }
    const _errs3 = errors;
    if (!validate31(data, { instancePath, parentData, parentDataProperty, rootData, dynamicAnchors })) {
      vErrors = vErrors === null ? validate31.errors : vErrors.concat(validate31.errors);
      errors = vErrors.length;
    }
    var _valid0 = _errs3 === errors;
    if (_valid0 && valid0) {
      valid0 = false;
      passing0 = [passing0, 2];
    } else {
      if (_valid0) {
        valid0 = true;
        passing0 = 2;
        if (props0 !== true) {
          props0 = true;
        }
      }
      const _errs4 = errors;
      if (!validate34(data, { instancePath, parentData, parentDataProperty, rootData, dynamicAnchors })) {
        vErrors = vErrors === null ? validate34.errors : vErrors.concat(validate34.errors);
        errors = vErrors.length;
      }
      var _valid0 = _errs4 === errors;
      if (_valid0 && valid0) {
        valid0 = false;
        passing0 = [passing0, 3];
      } else {
        if (_valid0) {
          valid0 = true;
          passing0 = 3;
          if (props0 !== true) {
            props0 = true;
          }
        }
        const _errs5 = errors;
        if (!validate37(data, { instancePath, parentData, parentDataProperty, rootData, dynamicAnchors })) {
          vErrors = vErrors === null ? validate37.errors : vErrors.concat(validate37.errors);
          errors = vErrors.length;
        }
        var _valid0 = _errs5 === errors;
        if (_valid0 && valid0) {
          valid0 = false;
          passing0 = [passing0, 4];
        } else {
          if (_valid0) {
            valid0 = true;
            passing0 = 4;
            if (props0 !== true) {
              props0 = true;
            }
          }
          const _errs6 = errors;
          if (!validate40(data, { instancePath, parentData, parentDataProperty, rootData, dynamicAnchors })) {
            vErrors = vErrors === null ? validate40.errors : vErrors.concat(validate40.errors);
            errors = vErrors.length;
          }
          var _valid0 = _errs6 === errors;
          if (_valid0 && valid0) {
            valid0 = false;
            passing0 = [passing0, 5];
          } else {
            if (_valid0) {
              valid0 = true;
              passing0 = 5;
              if (props0 !== true) {
                props0 = true;
              }
            }
          }
        }
      }
    }
  }
  if (!valid0) {
    const err0 = { instancePath, schemaPath: "#/oneOf", keyword: "oneOf", params: { passingSchemas: passing0 }, message: "must match exactly one schema in oneOf" };
    if (vErrors === null) {
      vErrors = [err0];
    } else {
      vErrors.push(err0);
    }
    errors++;
    validate21.errors = vErrors;
    return false;
  } else {
    errors = _errs0;
    if (vErrors !== null) {
      if (_errs0) {
        vErrors.length = _errs0;
      } else {
        vErrors = null;
      }
    }
  }
  validate21.errors = vErrors;
  evaluated0.props = props0;
  return errors === 0;
}
validate21.evaluated = { "dynamicProps": true, "dynamicItems": false };
var schema57 = { "title": "DashboardQueryResponseV1", "oneOf": [{ "$ref": "#/$defs/bootstrap_response" }, { "$ref": "#/$defs/traces_response" }, { "$ref": "#/$defs/spans_response" }, { "$ref": "#/$defs/summary_response" }, { "$ref": "#/$defs/span_response" }, { "$ref": "#/$defs/facets_response" }, { "$ref": "#/$defs/status_response" }] };
var schema58 = { "title": "DashboardBootstrapResponseV1", "type": "object", "allOf": [{ "$ref": "#/$defs/response_base" }, { "type": "object", "required": ["kind", "limits"], "properties": { "kind": { "const": "bootstrap" }, "limits": { "$ref": "#/$defs/limits" } } }], "unevaluatedProperties": false };
var schema79 = { "title": "DashboardQueryLimitsV1", "type": "object", "additionalProperties": false, "required": ["traceRows", "spanRows", "timelineRows", "facetValues", "requestBytes", "responseBytes"], "properties": { "traceRows": { "const": 100 }, "spanRows": { "const": 200 }, "timelineRows": { "const": 120 }, "facetValues": { "const": 500 }, "requestBytes": { "const": 8192 }, "responseBytes": { "const": 1048576 } } };
var schema59 = { "type": "object", "required": ["schemaVersion", "snapshot", "scope", "work"], "properties": { "schemaVersion": { "const": "agent_observability.dashboard_query.v1" }, "snapshot": { "$ref": "#/$defs/snapshot" }, "scope": { "$ref": "#/$defs/scope" }, "work": { "$ref": "#/$defs/work" } } };
var schema60 = { "title": "DashboardSnapshotV1", "type": "object", "additionalProperties": false, "required": ["id", "generation", "visibilityEpoch", "generatedAt", "state"], "properties": { "id": { "$ref": "#/$defs/snapshot_id" }, "generation": { "$ref": "#/$defs/decimal_u64" }, "visibilityEpoch": { "$ref": "#/$defs/decimal_u64" }, "generatedAt": { "type": "string", "minLength": 1, "maxLength": 64 }, "state": { "$ref": "#/$defs/snapshot_state" } } };
var schema62 = { "type": "string", "minLength": 1, "maxLength": 20, "pattern": "^(0|[1-9][0-9]{0,19})$", "description": "Unsigned 64-bit counter serialized as a decimal string to avoid JavaScript precision loss. Semantic validators additionally reject values above u64::MAX." };
var schema64 = { "title": "DashboardSnapshotStateV1", "type": "string", "enum": ["current", "stale"] };
var pattern13 = new RegExp("^(0|[1-9][0-9]{0,19})$", "u");
function validate47(data, { instancePath = "", parentData, parentDataProperty, rootData = data, dynamicAnchors = {} } = {}) {
  let vErrors = null;
  let errors = 0;
  const evaluated0 = validate47.evaluated;
  if (evaluated0.dynamicProps) {
    evaluated0.props = void 0;
  }
  if (evaluated0.dynamicItems) {
    evaluated0.items = void 0;
  }
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.id === void 0 && (missing0 = "id") || data.generation === void 0 && (missing0 = "generation") || data.visibilityEpoch === void 0 && (missing0 = "visibilityEpoch") || data.generatedAt === void 0 && (missing0 = "generatedAt") || data.state === void 0 && (missing0 = "state")) {
        validate47.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "id" || key0 === "generation" || key0 === "visibilityEpoch" || key0 === "generatedAt" || key0 === "state")) {
            validate47.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.id !== void 0) {
            let data0 = data.id;
            const _errs2 = errors;
            const _errs3 = errors;
            if (errors === _errs3) {
              if (typeof data0 === "string") {
                if (!pattern4.test(data0)) {
                  validate47.errors = [{ instancePath: instancePath + "/id", schemaPath: "#/$defs/snapshot_id/pattern", keyword: "pattern", params: { pattern: "^[0-9a-f]{64}$" }, message: 'must match pattern "^[0-9a-f]{64}$"' }];
                  return false;
                }
              } else {
                validate47.errors = [{ instancePath: instancePath + "/id", schemaPath: "#/$defs/snapshot_id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                return false;
              }
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.generation !== void 0) {
              let data1 = data.generation;
              const _errs5 = errors;
              const _errs6 = errors;
              if (errors === _errs6) {
                if (typeof data1 === "string") {
                  if (func1(data1) > 20) {
                    validate47.errors = [{ instancePath: instancePath + "/generation", schemaPath: "#/$defs/decimal_u64/maxLength", keyword: "maxLength", params: { limit: 20 }, message: "must NOT have more than 20 characters" }];
                    return false;
                  } else {
                    if (func1(data1) < 1) {
                      validate47.errors = [{ instancePath: instancePath + "/generation", schemaPath: "#/$defs/decimal_u64/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                      return false;
                    } else {
                      if (!pattern13.test(data1)) {
                        validate47.errors = [{ instancePath: instancePath + "/generation", schemaPath: "#/$defs/decimal_u64/pattern", keyword: "pattern", params: { pattern: "^(0|[1-9][0-9]{0,19})$" }, message: 'must match pattern "^(0|[1-9][0-9]{0,19})$"' }];
                        return false;
                      }
                    }
                  }
                } else {
                  validate47.errors = [{ instancePath: instancePath + "/generation", schemaPath: "#/$defs/decimal_u64/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                  return false;
                }
              }
              var valid0 = _errs5 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.visibilityEpoch !== void 0) {
                let data2 = data.visibilityEpoch;
                const _errs8 = errors;
                const _errs9 = errors;
                if (errors === _errs9) {
                  if (typeof data2 === "string") {
                    if (func1(data2) > 20) {
                      validate47.errors = [{ instancePath: instancePath + "/visibilityEpoch", schemaPath: "#/$defs/decimal_u64/maxLength", keyword: "maxLength", params: { limit: 20 }, message: "must NOT have more than 20 characters" }];
                      return false;
                    } else {
                      if (func1(data2) < 1) {
                        validate47.errors = [{ instancePath: instancePath + "/visibilityEpoch", schemaPath: "#/$defs/decimal_u64/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                        return false;
                      } else {
                        if (!pattern13.test(data2)) {
                          validate47.errors = [{ instancePath: instancePath + "/visibilityEpoch", schemaPath: "#/$defs/decimal_u64/pattern", keyword: "pattern", params: { pattern: "^(0|[1-9][0-9]{0,19})$" }, message: 'must match pattern "^(0|[1-9][0-9]{0,19})$"' }];
                          return false;
                        }
                      }
                    }
                  } else {
                    validate47.errors = [{ instancePath: instancePath + "/visibilityEpoch", schemaPath: "#/$defs/decimal_u64/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                    return false;
                  }
                }
                var valid0 = _errs8 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.generatedAt !== void 0) {
                  let data3 = data.generatedAt;
                  const _errs11 = errors;
                  if (errors === _errs11) {
                    if (typeof data3 === "string") {
                      if (func1(data3) > 64) {
                        validate47.errors = [{ instancePath: instancePath + "/generatedAt", schemaPath: "#/properties/generatedAt/maxLength", keyword: "maxLength", params: { limit: 64 }, message: "must NOT have more than 64 characters" }];
                        return false;
                      } else {
                        if (func1(data3) < 1) {
                          validate47.errors = [{ instancePath: instancePath + "/generatedAt", schemaPath: "#/properties/generatedAt/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                          return false;
                        }
                      }
                    } else {
                      validate47.errors = [{ instancePath: instancePath + "/generatedAt", schemaPath: "#/properties/generatedAt/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                      return false;
                    }
                  }
                  var valid0 = _errs11 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.state !== void 0) {
                    let data4 = data.state;
                    const _errs13 = errors;
                    if (typeof data4 !== "string") {
                      validate47.errors = [{ instancePath: instancePath + "/state", schemaPath: "#/$defs/snapshot_state/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                      return false;
                    }
                    if (!(data4 === "current" || data4 === "stale")) {
                      validate47.errors = [{ instancePath: instancePath + "/state", schemaPath: "#/$defs/snapshot_state/enum", keyword: "enum", params: { allowedValues: schema64.enum }, message: "must be equal to one of the allowed values" }];
                      return false;
                    }
                    var valid0 = _errs13 === errors;
                  } else {
                    var valid0 = true;
                  }
                }
              }
            }
          }
        }
      }
    } else {
      validate47.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate47.errors = vErrors;
  return errors === 0;
}
validate47.evaluated = { "props": true, "dynamicProps": false, "dynamicItems": false };
var schema65 = { "title": "DashboardQueryScopeV1", "type": "object", "additionalProperties": false, "required": ["filters", "selectedTraceId", "coldExcluded"], "properties": { "filters": { "$ref": "#/$defs/filters" }, "selectedTraceId": { "oneOf": [{ "$ref": "#/$defs/projected_id" }, { "type": "null" }] }, "coldExcluded": { "const": true } } };
function validate49(data, { instancePath = "", parentData, parentDataProperty, rootData = data, dynamicAnchors = {} } = {}) {
  let vErrors = null;
  let errors = 0;
  const evaluated0 = validate49.evaluated;
  if (evaluated0.dynamicProps) {
    evaluated0.props = void 0;
  }
  if (evaluated0.dynamicItems) {
    evaluated0.items = void 0;
  }
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.filters === void 0 && (missing0 = "filters") || data.selectedTraceId === void 0 && (missing0 = "selectedTraceId") || data.coldExcluded === void 0 && (missing0 = "coldExcluded")) {
        validate49.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "filters" || key0 === "selectedTraceId" || key0 === "coldExcluded")) {
            validate49.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.filters !== void 0) {
            const _errs2 = errors;
            if (!validate24(data.filters, { instancePath: instancePath + "/filters", parentData: data, parentDataProperty: "filters", rootData, dynamicAnchors })) {
              vErrors = vErrors === null ? validate24.errors : vErrors.concat(validate24.errors);
              errors = vErrors.length;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.selectedTraceId !== void 0) {
              let data1 = data.selectedTraceId;
              const _errs3 = errors;
              const _errs4 = errors;
              let valid1 = false;
              let passing0 = null;
              const _errs5 = errors;
              const _errs6 = errors;
              if (errors === _errs6) {
                if (typeof data1 === "string") {
                  if (!pattern6.test(data1)) {
                    const err0 = { instancePath: instancePath + "/selectedTraceId", schemaPath: "#/$defs/projected_id/pattern", keyword: "pattern", params: { pattern: "^id:sha256:[0-9a-f]{64}$" }, message: 'must match pattern "^id:sha256:[0-9a-f]{64}$"' };
                    if (vErrors === null) {
                      vErrors = [err0];
                    } else {
                      vErrors.push(err0);
                    }
                    errors++;
                  }
                } else {
                  const err1 = { instancePath: instancePath + "/selectedTraceId", schemaPath: "#/$defs/projected_id/type", keyword: "type", params: { type: "string" }, message: "must be string" };
                  if (vErrors === null) {
                    vErrors = [err1];
                  } else {
                    vErrors.push(err1);
                  }
                  errors++;
                }
              }
              var _valid0 = _errs5 === errors;
              if (_valid0) {
                valid1 = true;
                passing0 = 0;
              }
              const _errs8 = errors;
              if (data1 !== null) {
                const err2 = { instancePath: instancePath + "/selectedTraceId", schemaPath: "#/properties/selectedTraceId/oneOf/1/type", keyword: "type", params: { type: "null" }, message: "must be null" };
                if (vErrors === null) {
                  vErrors = [err2];
                } else {
                  vErrors.push(err2);
                }
                errors++;
              }
              var _valid0 = _errs8 === errors;
              if (_valid0 && valid1) {
                valid1 = false;
                passing0 = [passing0, 1];
              } else {
                if (_valid0) {
                  valid1 = true;
                  passing0 = 1;
                }
              }
              if (!valid1) {
                const err3 = { instancePath: instancePath + "/selectedTraceId", schemaPath: "#/properties/selectedTraceId/oneOf", keyword: "oneOf", params: { passingSchemas: passing0 }, message: "must match exactly one schema in oneOf" };
                if (vErrors === null) {
                  vErrors = [err3];
                } else {
                  vErrors.push(err3);
                }
                errors++;
                validate49.errors = vErrors;
                return false;
              } else {
                errors = _errs4;
                if (vErrors !== null) {
                  if (_errs4) {
                    vErrors.length = _errs4;
                  } else {
                    vErrors = null;
                  }
                }
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.coldExcluded !== void 0) {
                const _errs10 = errors;
                if (true !== data.coldExcluded) {
                  validate49.errors = [{ instancePath: instancePath + "/coldExcluded", schemaPath: "#/properties/coldExcluded/const", keyword: "const", params: { allowedValue: true }, message: "must be equal to constant" }];
                  return false;
                }
                var valid0 = _errs10 === errors;
              } else {
                var valid0 = true;
              }
            }
          }
        }
      }
    } else {
      validate49.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate49.errors = vErrors;
  return errors === 0;
}
validate49.evaluated = { "props": true, "dynamicProps": false, "dynamicItems": false };
var schema67 = { "title": "DashboardWorkV1", "description": "KPI values have no independent generation field and therefore inherit the enclosing snapshot generation by construction.", "oneOf": [{ "title": "DashboardPendingWorkV1", "type": "object", "additionalProperties": false, "required": ["state"], "properties": { "state": { "const": "pending" } } }, { "title": "DashboardCompleteWorkV1", "type": "object", "additionalProperties": false, "required": ["state", "kpis"], "properties": { "state": { "const": "complete" }, "kpis": { "$ref": "#/$defs/kpis" } } }] };
var schema68 = { "title": "DashboardKpisV1", "type": "object", "additionalProperties": false, "required": ["sessions", "turns", "llm", "tools", "errors", "inputTokens", "outputTokens", "totalTokens", "tokenStatus", "estimatedCost", "costStatus", "currency"], "properties": { "sessions": { "$ref": "#/$defs/safe_count" }, "turns": { "$ref": "#/$defs/safe_count" }, "llm": { "$ref": "#/$defs/safe_count" }, "tools": { "$ref": "#/$defs/safe_count" }, "errors": { "$ref": "#/$defs/safe_count" }, "inputTokens": { "$ref": "#/$defs/nullable_safe_count" }, "outputTokens": { "$ref": "#/$defs/nullable_safe_count" }, "totalTokens": { "$ref": "#/$defs/nullable_safe_count" }, "tokenStatus": { "$ref": "#/$defs/token_status" }, "estimatedCost": { "oneOf": [{ "type": "number", "minimum": 0, "maximum": 1e12 }, { "type": "null" }] }, "costStatus": { "$ref": "#/$defs/cost_status" }, "currency": { "oneOf": [{ "type": "string", "minLength": 1, "maxLength": 16 }, { "type": "null" }] } }, "allOf": [{ "oneOf": [{ "properties": { "tokenStatus": { "const": "complete" }, "totalTokens": { "$ref": "#/$defs/safe_count" } } }, { "properties": { "tokenStatus": { "const": "incomplete" }, "totalTokens": { "type": "null" } } }, { "properties": { "tokenStatus": { "const": "unavailable" }, "inputTokens": { "type": "null" }, "outputTokens": { "type": "null" }, "totalTokens": { "type": "null" } } }] }, { "oneOf": [{ "properties": { "costStatus": { "const": "estimated" }, "estimatedCost": { "type": "number", "minimum": 0, "maximum": 1e12 }, "currency": { "type": "string", "minLength": 1, "maxLength": 16 } } }, { "properties": { "costStatus": { "const": "incomplete" } } }, { "properties": { "costStatus": { "const": "unknown" }, "estimatedCost": { "type": "null" }, "currency": { "type": "null" } } }] }] };
var schema69 = { "type": "integer", "minimum": 0, "maximum": 9007199254740991 };
var schema77 = { "title": "DashboardTokenStatusV1", "type": "string", "enum": ["complete", "incomplete", "unavailable"] };
var schema78 = { "title": "DashboardCostStatusV1", "type": "string", "enum": ["estimated", "incomplete", "unknown"] };
var func27 = Object.prototype.hasOwnProperty;
var schema75 = { "oneOf": [{ "$ref": "#/$defs/safe_count" }, { "type": "null" }] };
function validate54(data, { instancePath = "", parentData, parentDataProperty, rootData = data, dynamicAnchors = {} } = {}) {
  let vErrors = null;
  let errors = 0;
  const evaluated0 = validate54.evaluated;
  if (evaluated0.dynamicProps) {
    evaluated0.props = void 0;
  }
  if (evaluated0.dynamicItems) {
    evaluated0.items = void 0;
  }
  const _errs0 = errors;
  let valid0 = false;
  let passing0 = null;
  const _errs1 = errors;
  const _errs2 = errors;
  if (!(typeof data == "number" && (!(data % 1) && !isNaN(data)) && isFinite(data))) {
    const err0 = { instancePath, schemaPath: "#/$defs/safe_count/type", keyword: "type", params: { type: "integer" }, message: "must be integer" };
    if (vErrors === null) {
      vErrors = [err0];
    } else {
      vErrors.push(err0);
    }
    errors++;
  }
  if (errors === _errs2) {
    if (typeof data == "number" && isFinite(data)) {
      if (data > 9007199254740991 || isNaN(data)) {
        const err1 = { instancePath, schemaPath: "#/$defs/safe_count/maximum", keyword: "maximum", params: { comparison: "<=", limit: 9007199254740991 }, message: "must be <= 9007199254740991" };
        if (vErrors === null) {
          vErrors = [err1];
        } else {
          vErrors.push(err1);
        }
        errors++;
      } else {
        if (data < 0 || isNaN(data)) {
          const err2 = { instancePath, schemaPath: "#/$defs/safe_count/minimum", keyword: "minimum", params: { comparison: ">=", limit: 0 }, message: "must be >= 0" };
          if (vErrors === null) {
            vErrors = [err2];
          } else {
            vErrors.push(err2);
          }
          errors++;
        }
      }
    }
  }
  var _valid0 = _errs1 === errors;
  if (_valid0) {
    valid0 = true;
    passing0 = 0;
  }
  const _errs4 = errors;
  if (data !== null) {
    const err3 = { instancePath, schemaPath: "#/oneOf/1/type", keyword: "type", params: { type: "null" }, message: "must be null" };
    if (vErrors === null) {
      vErrors = [err3];
    } else {
      vErrors.push(err3);
    }
    errors++;
  }
  var _valid0 = _errs4 === errors;
  if (_valid0 && valid0) {
    valid0 = false;
    passing0 = [passing0, 1];
  } else {
    if (_valid0) {
      valid0 = true;
      passing0 = 1;
    }
  }
  if (!valid0) {
    const err4 = { instancePath, schemaPath: "#/oneOf", keyword: "oneOf", params: { passingSchemas: passing0 }, message: "must match exactly one schema in oneOf" };
    if (vErrors === null) {
      vErrors = [err4];
    } else {
      vErrors.push(err4);
    }
    errors++;
    validate54.errors = vErrors;
    return false;
  } else {
    errors = _errs0;
    if (vErrors !== null) {
      if (_errs0) {
        vErrors.length = _errs0;
      } else {
        vErrors = null;
      }
    }
  }
  validate54.errors = vErrors;
  return errors === 0;
}
validate54.evaluated = { "dynamicProps": false, "dynamicItems": false };
function validate53(data, { instancePath = "", parentData, parentDataProperty, rootData = data, dynamicAnchors = {} } = {}) {
  let vErrors = null;
  let errors = 0;
  const evaluated0 = validate53.evaluated;
  if (evaluated0.dynamicProps) {
    evaluated0.props = void 0;
  }
  if (evaluated0.dynamicItems) {
    evaluated0.items = void 0;
  }
  const _errs1 = errors;
  const _errs2 = errors;
  let valid1 = false;
  let passing0 = null;
  const _errs3 = errors;
  if (data && typeof data == "object" && !Array.isArray(data)) {
    if (data.tokenStatus !== void 0) {
      const _errs4 = errors;
      if ("complete" !== data.tokenStatus) {
        const err0 = { instancePath: instancePath + "/tokenStatus", schemaPath: "#/allOf/0/oneOf/0/properties/tokenStatus/const", keyword: "const", params: { allowedValue: "complete" }, message: "must be equal to constant" };
        if (vErrors === null) {
          vErrors = [err0];
        } else {
          vErrors.push(err0);
        }
        errors++;
      }
      var valid2 = _errs4 === errors;
    } else {
      var valid2 = true;
    }
    if (valid2) {
      if (data.totalTokens !== void 0) {
        let data1 = data.totalTokens;
        const _errs5 = errors;
        const _errs6 = errors;
        if (!(typeof data1 == "number" && (!(data1 % 1) && !isNaN(data1)) && isFinite(data1))) {
          const err1 = { instancePath: instancePath + "/totalTokens", schemaPath: "#/$defs/safe_count/type", keyword: "type", params: { type: "integer" }, message: "must be integer" };
          if (vErrors === null) {
            vErrors = [err1];
          } else {
            vErrors.push(err1);
          }
          errors++;
        }
        if (errors === _errs6) {
          if (typeof data1 == "number" && isFinite(data1)) {
            if (data1 > 9007199254740991 || isNaN(data1)) {
              const err2 = { instancePath: instancePath + "/totalTokens", schemaPath: "#/$defs/safe_count/maximum", keyword: "maximum", params: { comparison: "<=", limit: 9007199254740991 }, message: "must be <= 9007199254740991" };
              if (vErrors === null) {
                vErrors = [err2];
              } else {
                vErrors.push(err2);
              }
              errors++;
            } else {
              if (data1 < 0 || isNaN(data1)) {
                const err3 = { instancePath: instancePath + "/totalTokens", schemaPath: "#/$defs/safe_count/minimum", keyword: "minimum", params: { comparison: ">=", limit: 0 }, message: "must be >= 0" };
                if (vErrors === null) {
                  vErrors = [err3];
                } else {
                  vErrors.push(err3);
                }
                errors++;
              }
            }
          }
        }
        var valid2 = _errs5 === errors;
      } else {
        var valid2 = true;
      }
    }
  }
  var _valid0 = _errs3 === errors;
  if (_valid0) {
    valid1 = true;
    passing0 = 0;
    var props0 = {};
    props0.tokenStatus = true;
    props0.totalTokens = true;
  }
  const _errs8 = errors;
  if (data && typeof data == "object" && !Array.isArray(data)) {
    if (data.tokenStatus !== void 0) {
      const _errs9 = errors;
      if ("incomplete" !== data.tokenStatus) {
        const err4 = { instancePath: instancePath + "/tokenStatus", schemaPath: "#/allOf/0/oneOf/1/properties/tokenStatus/const", keyword: "const", params: { allowedValue: "incomplete" }, message: "must be equal to constant" };
        if (vErrors === null) {
          vErrors = [err4];
        } else {
          vErrors.push(err4);
        }
        errors++;
      }
      var valid4 = _errs9 === errors;
    } else {
      var valid4 = true;
    }
    if (valid4) {
      if (data.totalTokens !== void 0) {
        const _errs10 = errors;
        if (data.totalTokens !== null) {
          const err5 = { instancePath: instancePath + "/totalTokens", schemaPath: "#/allOf/0/oneOf/1/properties/totalTokens/type", keyword: "type", params: { type: "null" }, message: "must be null" };
          if (vErrors === null) {
            vErrors = [err5];
          } else {
            vErrors.push(err5);
          }
          errors++;
        }
        var valid4 = _errs10 === errors;
      } else {
        var valid4 = true;
      }
    }
  }
  var _valid0 = _errs8 === errors;
  if (_valid0 && valid1) {
    valid1 = false;
    passing0 = [passing0, 1];
  } else {
    if (_valid0) {
      valid1 = true;
      passing0 = 1;
      if (props0 !== true) {
        props0 = props0 || {};
        props0.tokenStatus = true;
        props0.totalTokens = true;
      }
    }
    const _errs12 = errors;
    if (data && typeof data == "object" && !Array.isArray(data)) {
      if (data.tokenStatus !== void 0) {
        const _errs13 = errors;
        if ("unavailable" !== data.tokenStatus) {
          const err6 = { instancePath: instancePath + "/tokenStatus", schemaPath: "#/allOf/0/oneOf/2/properties/tokenStatus/const", keyword: "const", params: { allowedValue: "unavailable" }, message: "must be equal to constant" };
          if (vErrors === null) {
            vErrors = [err6];
          } else {
            vErrors.push(err6);
          }
          errors++;
        }
        var valid5 = _errs13 === errors;
      } else {
        var valid5 = true;
      }
      if (valid5) {
        if (data.inputTokens !== void 0) {
          const _errs14 = errors;
          if (data.inputTokens !== null) {
            const err7 = { instancePath: instancePath + "/inputTokens", schemaPath: "#/allOf/0/oneOf/2/properties/inputTokens/type", keyword: "type", params: { type: "null" }, message: "must be null" };
            if (vErrors === null) {
              vErrors = [err7];
            } else {
              vErrors.push(err7);
            }
            errors++;
          }
          var valid5 = _errs14 === errors;
        } else {
          var valid5 = true;
        }
        if (valid5) {
          if (data.outputTokens !== void 0) {
            const _errs16 = errors;
            if (data.outputTokens !== null) {
              const err8 = { instancePath: instancePath + "/outputTokens", schemaPath: "#/allOf/0/oneOf/2/properties/outputTokens/type", keyword: "type", params: { type: "null" }, message: "must be null" };
              if (vErrors === null) {
                vErrors = [err8];
              } else {
                vErrors.push(err8);
              }
              errors++;
            }
            var valid5 = _errs16 === errors;
          } else {
            var valid5 = true;
          }
          if (valid5) {
            if (data.totalTokens !== void 0) {
              const _errs18 = errors;
              if (data.totalTokens !== null) {
                const err9 = { instancePath: instancePath + "/totalTokens", schemaPath: "#/allOf/0/oneOf/2/properties/totalTokens/type", keyword: "type", params: { type: "null" }, message: "must be null" };
                if (vErrors === null) {
                  vErrors = [err9];
                } else {
                  vErrors.push(err9);
                }
                errors++;
              }
              var valid5 = _errs18 === errors;
            } else {
              var valid5 = true;
            }
          }
        }
      }
    }
    var _valid0 = _errs12 === errors;
    if (_valid0 && valid1) {
      valid1 = false;
      passing0 = [passing0, 2];
    } else {
      if (_valid0) {
        valid1 = true;
        passing0 = 2;
        if (props0 !== true) {
          props0 = props0 || {};
          props0.tokenStatus = true;
          props0.inputTokens = true;
          props0.outputTokens = true;
          props0.totalTokens = true;
        }
      }
    }
  }
  if (!valid1) {
    const err10 = { instancePath, schemaPath: "#/allOf/0/oneOf", keyword: "oneOf", params: { passingSchemas: passing0 }, message: "must match exactly one schema in oneOf" };
    if (vErrors === null) {
      vErrors = [err10];
    } else {
      vErrors.push(err10);
    }
    errors++;
    validate53.errors = vErrors;
    return false;
  } else {
    errors = _errs2;
    if (vErrors !== null) {
      if (_errs2) {
        vErrors.length = _errs2;
      } else {
        vErrors = null;
      }
    }
  }
  var valid0 = _errs1 === errors;
  if (valid0) {
    const _errs20 = errors;
    const _errs21 = errors;
    let valid6 = false;
    let passing1 = null;
    const _errs22 = errors;
    if (data && typeof data == "object" && !Array.isArray(data)) {
      if (data.costStatus !== void 0) {
        const _errs23 = errors;
        if ("estimated" !== data.costStatus) {
          const err11 = { instancePath: instancePath + "/costStatus", schemaPath: "#/allOf/1/oneOf/0/properties/costStatus/const", keyword: "const", params: { allowedValue: "estimated" }, message: "must be equal to constant" };
          if (vErrors === null) {
            vErrors = [err11];
          } else {
            vErrors.push(err11);
          }
          errors++;
        }
        var valid7 = _errs23 === errors;
      } else {
        var valid7 = true;
      }
      if (valid7) {
        if (data.estimatedCost !== void 0) {
          let data9 = data.estimatedCost;
          const _errs24 = errors;
          if (errors === _errs24) {
            if (typeof data9 == "number" && isFinite(data9)) {
              if (data9 > 1e12 || isNaN(data9)) {
                const err12 = { instancePath: instancePath + "/estimatedCost", schemaPath: "#/allOf/1/oneOf/0/properties/estimatedCost/maximum", keyword: "maximum", params: { comparison: "<=", limit: 1e12 }, message: "must be <= 1000000000000" };
                if (vErrors === null) {
                  vErrors = [err12];
                } else {
                  vErrors.push(err12);
                }
                errors++;
              } else {
                if (data9 < 0 || isNaN(data9)) {
                  const err13 = { instancePath: instancePath + "/estimatedCost", schemaPath: "#/allOf/1/oneOf/0/properties/estimatedCost/minimum", keyword: "minimum", params: { comparison: ">=", limit: 0 }, message: "must be >= 0" };
                  if (vErrors === null) {
                    vErrors = [err13];
                  } else {
                    vErrors.push(err13);
                  }
                  errors++;
                }
              }
            } else {
              const err14 = { instancePath: instancePath + "/estimatedCost", schemaPath: "#/allOf/1/oneOf/0/properties/estimatedCost/type", keyword: "type", params: { type: "number" }, message: "must be number" };
              if (vErrors === null) {
                vErrors = [err14];
              } else {
                vErrors.push(err14);
              }
              errors++;
            }
          }
          var valid7 = _errs24 === errors;
        } else {
          var valid7 = true;
        }
        if (valid7) {
          if (data.currency !== void 0) {
            let data10 = data.currency;
            const _errs26 = errors;
            if (errors === _errs26) {
              if (typeof data10 === "string") {
                if (func1(data10) > 16) {
                  const err15 = { instancePath: instancePath + "/currency", schemaPath: "#/allOf/1/oneOf/0/properties/currency/maxLength", keyword: "maxLength", params: { limit: 16 }, message: "must NOT have more than 16 characters" };
                  if (vErrors === null) {
                    vErrors = [err15];
                  } else {
                    vErrors.push(err15);
                  }
                  errors++;
                } else {
                  if (func1(data10) < 1) {
                    const err16 = { instancePath: instancePath + "/currency", schemaPath: "#/allOf/1/oneOf/0/properties/currency/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" };
                    if (vErrors === null) {
                      vErrors = [err16];
                    } else {
                      vErrors.push(err16);
                    }
                    errors++;
                  }
                }
              } else {
                const err17 = { instancePath: instancePath + "/currency", schemaPath: "#/allOf/1/oneOf/0/properties/currency/type", keyword: "type", params: { type: "string" }, message: "must be string" };
                if (vErrors === null) {
                  vErrors = [err17];
                } else {
                  vErrors.push(err17);
                }
                errors++;
              }
            }
            var valid7 = _errs26 === errors;
          } else {
            var valid7 = true;
          }
        }
      }
    }
    var _valid1 = _errs22 === errors;
    if (_valid1) {
      valid6 = true;
      passing1 = 0;
      var props1 = {};
      props1.costStatus = true;
      props1.estimatedCost = true;
      props1.currency = true;
    }
    const _errs28 = errors;
    if (data && typeof data == "object" && !Array.isArray(data)) {
      if (data.costStatus !== void 0) {
        if ("incomplete" !== data.costStatus) {
          const err18 = { instancePath: instancePath + "/costStatus", schemaPath: "#/allOf/1/oneOf/1/properties/costStatus/const", keyword: "const", params: { allowedValue: "incomplete" }, message: "must be equal to constant" };
          if (vErrors === null) {
            vErrors = [err18];
          } else {
            vErrors.push(err18);
          }
          errors++;
        }
      }
    }
    var _valid1 = _errs28 === errors;
    if (_valid1 && valid6) {
      valid6 = false;
      passing1 = [passing1, 1];
    } else {
      if (_valid1) {
        valid6 = true;
        passing1 = 1;
        if (props1 !== true) {
          props1 = props1 || {};
          props1.costStatus = true;
        }
      }
      const _errs30 = errors;
      if (data && typeof data == "object" && !Array.isArray(data)) {
        if (data.costStatus !== void 0) {
          const _errs31 = errors;
          if ("unknown" !== data.costStatus) {
            const err19 = { instancePath: instancePath + "/costStatus", schemaPath: "#/allOf/1/oneOf/2/properties/costStatus/const", keyword: "const", params: { allowedValue: "unknown" }, message: "must be equal to constant" };
            if (vErrors === null) {
              vErrors = [err19];
            } else {
              vErrors.push(err19);
            }
            errors++;
          }
          var valid9 = _errs31 === errors;
        } else {
          var valid9 = true;
        }
        if (valid9) {
          if (data.estimatedCost !== void 0) {
            const _errs32 = errors;
            if (data.estimatedCost !== null) {
              const err20 = { instancePath: instancePath + "/estimatedCost", schemaPath: "#/allOf/1/oneOf/2/properties/estimatedCost/type", keyword: "type", params: { type: "null" }, message: "must be null" };
              if (vErrors === null) {
                vErrors = [err20];
              } else {
                vErrors.push(err20);
              }
              errors++;
            }
            var valid9 = _errs32 === errors;
          } else {
            var valid9 = true;
          }
          if (valid9) {
            if (data.currency !== void 0) {
              const _errs34 = errors;
              if (data.currency !== null) {
                const err21 = { instancePath: instancePath + "/currency", schemaPath: "#/allOf/1/oneOf/2/properties/currency/type", keyword: "type", params: { type: "null" }, message: "must be null" };
                if (vErrors === null) {
                  vErrors = [err21];
                } else {
                  vErrors.push(err21);
                }
                errors++;
              }
              var valid9 = _errs34 === errors;
            } else {
              var valid9 = true;
            }
          }
        }
      }
      var _valid1 = _errs30 === errors;
      if (_valid1 && valid6) {
        valid6 = false;
        passing1 = [passing1, 2];
      } else {
        if (_valid1) {
          valid6 = true;
          passing1 = 2;
          if (props1 !== true) {
            props1 = props1 || {};
            props1.costStatus = true;
            props1.estimatedCost = true;
            props1.currency = true;
          }
        }
      }
    }
    if (!valid6) {
      const err22 = { instancePath, schemaPath: "#/allOf/1/oneOf", keyword: "oneOf", params: { passingSchemas: passing1 }, message: "must match exactly one schema in oneOf" };
      if (vErrors === null) {
        vErrors = [err22];
      } else {
        vErrors.push(err22);
      }
      errors++;
      validate53.errors = vErrors;
      return false;
    } else {
      errors = _errs21;
      if (vErrors !== null) {
        if (_errs21) {
          vErrors.length = _errs21;
        } else {
          vErrors = null;
        }
      }
    }
    var valid0 = _errs20 === errors;
    if (valid0) {
      if (props0 !== true && props1 !== void 0) {
        if (props1 === true) {
          props0 = true;
        } else {
          props0 = props0 || {};
          Object.assign(props0, props1);
        }
      }
    }
  }
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.sessions === void 0 && (missing0 = "sessions") || data.turns === void 0 && (missing0 = "turns") || data.llm === void 0 && (missing0 = "llm") || data.tools === void 0 && (missing0 = "tools") || data.errors === void 0 && (missing0 = "errors") || data.inputTokens === void 0 && (missing0 = "inputTokens") || data.outputTokens === void 0 && (missing0 = "outputTokens") || data.totalTokens === void 0 && (missing0 = "totalTokens") || data.tokenStatus === void 0 && (missing0 = "tokenStatus") || data.estimatedCost === void 0 && (missing0 = "estimatedCost") || data.costStatus === void 0 && (missing0 = "costStatus") || data.currency === void 0 && (missing0 = "currency")) {
        validate53.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs36 = errors;
        for (const key0 in data) {
          if (!func27.call(schema68.properties, key0)) {
            validate53.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs36 === errors) {
          if (data.sessions !== void 0) {
            let data15 = data.sessions;
            const _errs37 = errors;
            const _errs38 = errors;
            if (!(typeof data15 == "number" && (!(data15 % 1) && !isNaN(data15)) && isFinite(data15))) {
              validate53.errors = [{ instancePath: instancePath + "/sessions", schemaPath: "#/$defs/safe_count/type", keyword: "type", params: { type: "integer" }, message: "must be integer" }];
              return false;
            }
            if (errors === _errs38) {
              if (typeof data15 == "number" && isFinite(data15)) {
                if (data15 > 9007199254740991 || isNaN(data15)) {
                  validate53.errors = [{ instancePath: instancePath + "/sessions", schemaPath: "#/$defs/safe_count/maximum", keyword: "maximum", params: { comparison: "<=", limit: 9007199254740991 }, message: "must be <= 9007199254740991" }];
                  return false;
                } else {
                  if (data15 < 0 || isNaN(data15)) {
                    validate53.errors = [{ instancePath: instancePath + "/sessions", schemaPath: "#/$defs/safe_count/minimum", keyword: "minimum", params: { comparison: ">=", limit: 0 }, message: "must be >= 0" }];
                    return false;
                  }
                }
              }
            }
            var valid10 = _errs37 === errors;
          } else {
            var valid10 = true;
          }
          if (valid10) {
            if (data.turns !== void 0) {
              let data16 = data.turns;
              const _errs40 = errors;
              const _errs41 = errors;
              if (!(typeof data16 == "number" && (!(data16 % 1) && !isNaN(data16)) && isFinite(data16))) {
                validate53.errors = [{ instancePath: instancePath + "/turns", schemaPath: "#/$defs/safe_count/type", keyword: "type", params: { type: "integer" }, message: "must be integer" }];
                return false;
              }
              if (errors === _errs41) {
                if (typeof data16 == "number" && isFinite(data16)) {
                  if (data16 > 9007199254740991 || isNaN(data16)) {
                    validate53.errors = [{ instancePath: instancePath + "/turns", schemaPath: "#/$defs/safe_count/maximum", keyword: "maximum", params: { comparison: "<=", limit: 9007199254740991 }, message: "must be <= 9007199254740991" }];
                    return false;
                  } else {
                    if (data16 < 0 || isNaN(data16)) {
                      validate53.errors = [{ instancePath: instancePath + "/turns", schemaPath: "#/$defs/safe_count/minimum", keyword: "minimum", params: { comparison: ">=", limit: 0 }, message: "must be >= 0" }];
                      return false;
                    }
                  }
                }
              }
              var valid10 = _errs40 === errors;
            } else {
              var valid10 = true;
            }
            if (valid10) {
              if (data.llm !== void 0) {
                let data17 = data.llm;
                const _errs43 = errors;
                const _errs44 = errors;
                if (!(typeof data17 == "number" && (!(data17 % 1) && !isNaN(data17)) && isFinite(data17))) {
                  validate53.errors = [{ instancePath: instancePath + "/llm", schemaPath: "#/$defs/safe_count/type", keyword: "type", params: { type: "integer" }, message: "must be integer" }];
                  return false;
                }
                if (errors === _errs44) {
                  if (typeof data17 == "number" && isFinite(data17)) {
                    if (data17 > 9007199254740991 || isNaN(data17)) {
                      validate53.errors = [{ instancePath: instancePath + "/llm", schemaPath: "#/$defs/safe_count/maximum", keyword: "maximum", params: { comparison: "<=", limit: 9007199254740991 }, message: "must be <= 9007199254740991" }];
                      return false;
                    } else {
                      if (data17 < 0 || isNaN(data17)) {
                        validate53.errors = [{ instancePath: instancePath + "/llm", schemaPath: "#/$defs/safe_count/minimum", keyword: "minimum", params: { comparison: ">=", limit: 0 }, message: "must be >= 0" }];
                        return false;
                      }
                    }
                  }
                }
                var valid10 = _errs43 === errors;
              } else {
                var valid10 = true;
              }
              if (valid10) {
                if (data.tools !== void 0) {
                  let data18 = data.tools;
                  const _errs46 = errors;
                  const _errs47 = errors;
                  if (!(typeof data18 == "number" && (!(data18 % 1) && !isNaN(data18)) && isFinite(data18))) {
                    validate53.errors = [{ instancePath: instancePath + "/tools", schemaPath: "#/$defs/safe_count/type", keyword: "type", params: { type: "integer" }, message: "must be integer" }];
                    return false;
                  }
                  if (errors === _errs47) {
                    if (typeof data18 == "number" && isFinite(data18)) {
                      if (data18 > 9007199254740991 || isNaN(data18)) {
                        validate53.errors = [{ instancePath: instancePath + "/tools", schemaPath: "#/$defs/safe_count/maximum", keyword: "maximum", params: { comparison: "<=", limit: 9007199254740991 }, message: "must be <= 9007199254740991" }];
                        return false;
                      } else {
                        if (data18 < 0 || isNaN(data18)) {
                          validate53.errors = [{ instancePath: instancePath + "/tools", schemaPath: "#/$defs/safe_count/minimum", keyword: "minimum", params: { comparison: ">=", limit: 0 }, message: "must be >= 0" }];
                          return false;
                        }
                      }
                    }
                  }
                  var valid10 = _errs46 === errors;
                } else {
                  var valid10 = true;
                }
                if (valid10) {
                  if (data.errors !== void 0) {
                    let data19 = data.errors;
                    const _errs49 = errors;
                    const _errs50 = errors;
                    if (!(typeof data19 == "number" && (!(data19 % 1) && !isNaN(data19)) && isFinite(data19))) {
                      validate53.errors = [{ instancePath: instancePath + "/errors", schemaPath: "#/$defs/safe_count/type", keyword: "type", params: { type: "integer" }, message: "must be integer" }];
                      return false;
                    }
                    if (errors === _errs50) {
                      if (typeof data19 == "number" && isFinite(data19)) {
                        if (data19 > 9007199254740991 || isNaN(data19)) {
                          validate53.errors = [{ instancePath: instancePath + "/errors", schemaPath: "#/$defs/safe_count/maximum", keyword: "maximum", params: { comparison: "<=", limit: 9007199254740991 }, message: "must be <= 9007199254740991" }];
                          return false;
                        } else {
                          if (data19 < 0 || isNaN(data19)) {
                            validate53.errors = [{ instancePath: instancePath + "/errors", schemaPath: "#/$defs/safe_count/minimum", keyword: "minimum", params: { comparison: ">=", limit: 0 }, message: "must be >= 0" }];
                            return false;
                          }
                        }
                      }
                    }
                    var valid10 = _errs49 === errors;
                  } else {
                    var valid10 = true;
                  }
                  if (valid10) {
                    if (data.inputTokens !== void 0) {
                      const _errs52 = errors;
                      if (!validate54(data.inputTokens, { instancePath: instancePath + "/inputTokens", parentData: data, parentDataProperty: "inputTokens", rootData, dynamicAnchors })) {
                        vErrors = vErrors === null ? validate54.errors : vErrors.concat(validate54.errors);
                        errors = vErrors.length;
                      }
                      var valid10 = _errs52 === errors;
                    } else {
                      var valid10 = true;
                    }
                    if (valid10) {
                      if (data.outputTokens !== void 0) {
                        const _errs53 = errors;
                        if (!validate54(data.outputTokens, { instancePath: instancePath + "/outputTokens", parentData: data, parentDataProperty: "outputTokens", rootData, dynamicAnchors })) {
                          vErrors = vErrors === null ? validate54.errors : vErrors.concat(validate54.errors);
                          errors = vErrors.length;
                        }
                        var valid10 = _errs53 === errors;
                      } else {
                        var valid10 = true;
                      }
                      if (valid10) {
                        if (data.totalTokens !== void 0) {
                          const _errs54 = errors;
                          if (!validate54(data.totalTokens, { instancePath: instancePath + "/totalTokens", parentData: data, parentDataProperty: "totalTokens", rootData, dynamicAnchors })) {
                            vErrors = vErrors === null ? validate54.errors : vErrors.concat(validate54.errors);
                            errors = vErrors.length;
                          }
                          var valid10 = _errs54 === errors;
                        } else {
                          var valid10 = true;
                        }
                        if (valid10) {
                          if (data.tokenStatus !== void 0) {
                            let data23 = data.tokenStatus;
                            const _errs55 = errors;
                            if (typeof data23 !== "string") {
                              validate53.errors = [{ instancePath: instancePath + "/tokenStatus", schemaPath: "#/$defs/token_status/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                              return false;
                            }
                            if (!(data23 === "complete" || data23 === "incomplete" || data23 === "unavailable")) {
                              validate53.errors = [{ instancePath: instancePath + "/tokenStatus", schemaPath: "#/$defs/token_status/enum", keyword: "enum", params: { allowedValues: schema77.enum }, message: "must be equal to one of the allowed values" }];
                              return false;
                            }
                            var valid10 = _errs55 === errors;
                          } else {
                            var valid10 = true;
                          }
                          if (valid10) {
                            if (data.estimatedCost !== void 0) {
                              let data24 = data.estimatedCost;
                              const _errs58 = errors;
                              const _errs59 = errors;
                              let valid17 = false;
                              let passing2 = null;
                              const _errs60 = errors;
                              if (errors === _errs60) {
                                if (typeof data24 == "number" && isFinite(data24)) {
                                  if (data24 > 1e12 || isNaN(data24)) {
                                    const err23 = { instancePath: instancePath + "/estimatedCost", schemaPath: "#/properties/estimatedCost/oneOf/0/maximum", keyword: "maximum", params: { comparison: "<=", limit: 1e12 }, message: "must be <= 1000000000000" };
                                    if (vErrors === null) {
                                      vErrors = [err23];
                                    } else {
                                      vErrors.push(err23);
                                    }
                                    errors++;
                                  } else {
                                    if (data24 < 0 || isNaN(data24)) {
                                      const err24 = { instancePath: instancePath + "/estimatedCost", schemaPath: "#/properties/estimatedCost/oneOf/0/minimum", keyword: "minimum", params: { comparison: ">=", limit: 0 }, message: "must be >= 0" };
                                      if (vErrors === null) {
                                        vErrors = [err24];
                                      } else {
                                        vErrors.push(err24);
                                      }
                                      errors++;
                                    }
                                  }
                                } else {
                                  const err25 = { instancePath: instancePath + "/estimatedCost", schemaPath: "#/properties/estimatedCost/oneOf/0/type", keyword: "type", params: { type: "number" }, message: "must be number" };
                                  if (vErrors === null) {
                                    vErrors = [err25];
                                  } else {
                                    vErrors.push(err25);
                                  }
                                  errors++;
                                }
                              }
                              var _valid2 = _errs60 === errors;
                              if (_valid2) {
                                valid17 = true;
                                passing2 = 0;
                              }
                              const _errs62 = errors;
                              if (data24 !== null) {
                                const err26 = { instancePath: instancePath + "/estimatedCost", schemaPath: "#/properties/estimatedCost/oneOf/1/type", keyword: "type", params: { type: "null" }, message: "must be null" };
                                if (vErrors === null) {
                                  vErrors = [err26];
                                } else {
                                  vErrors.push(err26);
                                }
                                errors++;
                              }
                              var _valid2 = _errs62 === errors;
                              if (_valid2 && valid17) {
                                valid17 = false;
                                passing2 = [passing2, 1];
                              } else {
                                if (_valid2) {
                                  valid17 = true;
                                  passing2 = 1;
                                }
                              }
                              if (!valid17) {
                                const err27 = { instancePath: instancePath + "/estimatedCost", schemaPath: "#/properties/estimatedCost/oneOf", keyword: "oneOf", params: { passingSchemas: passing2 }, message: "must match exactly one schema in oneOf" };
                                if (vErrors === null) {
                                  vErrors = [err27];
                                } else {
                                  vErrors.push(err27);
                                }
                                errors++;
                                validate53.errors = vErrors;
                                return false;
                              } else {
                                errors = _errs59;
                                if (vErrors !== null) {
                                  if (_errs59) {
                                    vErrors.length = _errs59;
                                  } else {
                                    vErrors = null;
                                  }
                                }
                              }
                              var valid10 = _errs58 === errors;
                            } else {
                              var valid10 = true;
                            }
                            if (valid10) {
                              if (data.costStatus !== void 0) {
                                let data25 = data.costStatus;
                                const _errs64 = errors;
                                if (typeof data25 !== "string") {
                                  validate53.errors = [{ instancePath: instancePath + "/costStatus", schemaPath: "#/$defs/cost_status/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                  return false;
                                }
                                if (!(data25 === "estimated" || data25 === "incomplete" || data25 === "unknown")) {
                                  validate53.errors = [{ instancePath: instancePath + "/costStatus", schemaPath: "#/$defs/cost_status/enum", keyword: "enum", params: { allowedValues: schema78.enum }, message: "must be equal to one of the allowed values" }];
                                  return false;
                                }
                                var valid10 = _errs64 === errors;
                              } else {
                                var valid10 = true;
                              }
                              if (valid10) {
                                if (data.currency !== void 0) {
                                  let data26 = data.currency;
                                  const _errs67 = errors;
                                  const _errs68 = errors;
                                  let valid19 = false;
                                  let passing3 = null;
                                  const _errs69 = errors;
                                  if (errors === _errs69) {
                                    if (typeof data26 === "string") {
                                      if (func1(data26) > 16) {
                                        const err28 = { instancePath: instancePath + "/currency", schemaPath: "#/properties/currency/oneOf/0/maxLength", keyword: "maxLength", params: { limit: 16 }, message: "must NOT have more than 16 characters" };
                                        if (vErrors === null) {
                                          vErrors = [err28];
                                        } else {
                                          vErrors.push(err28);
                                        }
                                        errors++;
                                      } else {
                                        if (func1(data26) < 1) {
                                          const err29 = { instancePath: instancePath + "/currency", schemaPath: "#/properties/currency/oneOf/0/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" };
                                          if (vErrors === null) {
                                            vErrors = [err29];
                                          } else {
                                            vErrors.push(err29);
                                          }
                                          errors++;
                                        }
                                      }
                                    } else {
                                      const err30 = { instancePath: instancePath + "/currency", schemaPath: "#/properties/currency/oneOf/0/type", keyword: "type", params: { type: "string" }, message: "must be string" };
                                      if (vErrors === null) {
                                        vErrors = [err30];
                                      } else {
                                        vErrors.push(err30);
                                      }
                                      errors++;
                                    }
                                  }
                                  var _valid3 = _errs69 === errors;
                                  if (_valid3) {
                                    valid19 = true;
                                    passing3 = 0;
                                  }
                                  const _errs71 = errors;
                                  if (data26 !== null) {
                                    const err31 = { instancePath: instancePath + "/currency", schemaPath: "#/properties/currency/oneOf/1/type", keyword: "type", params: { type: "null" }, message: "must be null" };
                                    if (vErrors === null) {
                                      vErrors = [err31];
                                    } else {
                                      vErrors.push(err31);
                                    }
                                    errors++;
                                  }
                                  var _valid3 = _errs71 === errors;
                                  if (_valid3 && valid19) {
                                    valid19 = false;
                                    passing3 = [passing3, 1];
                                  } else {
                                    if (_valid3) {
                                      valid19 = true;
                                      passing3 = 1;
                                    }
                                  }
                                  if (!valid19) {
                                    const err32 = { instancePath: instancePath + "/currency", schemaPath: "#/properties/currency/oneOf", keyword: "oneOf", params: { passingSchemas: passing3 }, message: "must match exactly one schema in oneOf" };
                                    if (vErrors === null) {
                                      vErrors = [err32];
                                    } else {
                                      vErrors.push(err32);
                                    }
                                    errors++;
                                    validate53.errors = vErrors;
                                    return false;
                                  } else {
                                    errors = _errs68;
                                    if (vErrors !== null) {
                                      if (_errs68) {
                                        vErrors.length = _errs68;
                                      } else {
                                        vErrors = null;
                                      }
                                    }
                                  }
                                  var valid10 = _errs67 === errors;
                                } else {
                                  var valid10 = true;
                                }
                              }
                            }
                          }
                        }
                      }
                    }
                  }
                }
              }
            }
          }
        }
      }
    } else {
      validate53.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate53.errors = vErrors;
  return errors === 0;
}
validate53.evaluated = { "props": true, "dynamicProps": false, "dynamicItems": false };
function validate52(data, { instancePath = "", parentData, parentDataProperty, rootData = data, dynamicAnchors = {} } = {}) {
  let vErrors = null;
  let errors = 0;
  const evaluated0 = validate52.evaluated;
  if (evaluated0.dynamicProps) {
    evaluated0.props = void 0;
  }
  if (evaluated0.dynamicItems) {
    evaluated0.items = void 0;
  }
  const _errs0 = errors;
  let valid0 = false;
  let passing0 = null;
  const _errs1 = errors;
  if (errors === _errs1) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.state === void 0 && (missing0 = "state")) {
        const err0 = { instancePath, schemaPath: "#/oneOf/0/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" };
        if (vErrors === null) {
          vErrors = [err0];
        } else {
          vErrors.push(err0);
        }
        errors++;
      } else {
        const _errs3 = errors;
        for (const key0 in data) {
          if (!(key0 === "state")) {
            const err1 = { instancePath, schemaPath: "#/oneOf/0/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" };
            if (vErrors === null) {
              vErrors = [err1];
            } else {
              vErrors.push(err1);
            }
            errors++;
            break;
          }
        }
        if (_errs3 === errors) {
          if (data.state !== void 0) {
            if ("pending" !== data.state) {
              const err2 = { instancePath: instancePath + "/state", schemaPath: "#/oneOf/0/properties/state/const", keyword: "const", params: { allowedValue: "pending" }, message: "must be equal to constant" };
              if (vErrors === null) {
                vErrors = [err2];
              } else {
                vErrors.push(err2);
              }
              errors++;
            }
          }
        }
      }
    } else {
      const err3 = { instancePath, schemaPath: "#/oneOf/0/type", keyword: "type", params: { type: "object" }, message: "must be object" };
      if (vErrors === null) {
        vErrors = [err3];
      } else {
        vErrors.push(err3);
      }
      errors++;
    }
  }
  var _valid0 = _errs1 === errors;
  if (_valid0) {
    valid0 = true;
    passing0 = 0;
    var props0 = true;
  }
  const _errs5 = errors;
  if (errors === _errs5) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing1;
      if (data.state === void 0 && (missing1 = "state") || data.kpis === void 0 && (missing1 = "kpis")) {
        const err4 = { instancePath, schemaPath: "#/oneOf/1/required", keyword: "required", params: { missingProperty: missing1 }, message: "must have required property '" + missing1 + "'" };
        if (vErrors === null) {
          vErrors = [err4];
        } else {
          vErrors.push(err4);
        }
        errors++;
      } else {
        const _errs7 = errors;
        for (const key1 in data) {
          if (!(key1 === "state" || key1 === "kpis")) {
            const err5 = { instancePath, schemaPath: "#/oneOf/1/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key1 }, message: "must NOT have additional properties" };
            if (vErrors === null) {
              vErrors = [err5];
            } else {
              vErrors.push(err5);
            }
            errors++;
            break;
          }
        }
        if (_errs7 === errors) {
          if (data.state !== void 0) {
            const _errs8 = errors;
            if ("complete" !== data.state) {
              const err6 = { instancePath: instancePath + "/state", schemaPath: "#/oneOf/1/properties/state/const", keyword: "const", params: { allowedValue: "complete" }, message: "must be equal to constant" };
              if (vErrors === null) {
                vErrors = [err6];
              } else {
                vErrors.push(err6);
              }
              errors++;
            }
            var valid2 = _errs8 === errors;
          } else {
            var valid2 = true;
          }
          if (valid2) {
            if (data.kpis !== void 0) {
              const _errs9 = errors;
              if (!validate53(data.kpis, { instancePath: instancePath + "/kpis", parentData: data, parentDataProperty: "kpis", rootData, dynamicAnchors })) {
                vErrors = vErrors === null ? validate53.errors : vErrors.concat(validate53.errors);
                errors = vErrors.length;
              }
              var valid2 = _errs9 === errors;
            } else {
              var valid2 = true;
            }
          }
        }
      }
    } else {
      const err7 = { instancePath, schemaPath: "#/oneOf/1/type", keyword: "type", params: { type: "object" }, message: "must be object" };
      if (vErrors === null) {
        vErrors = [err7];
      } else {
        vErrors.push(err7);
      }
      errors++;
    }
  }
  var _valid0 = _errs5 === errors;
  if (_valid0 && valid0) {
    valid0 = false;
    passing0 = [passing0, 1];
  } else {
    if (_valid0) {
      valid0 = true;
      passing0 = 1;
      if (props0 !== true) {
        props0 = true;
      }
    }
  }
  if (!valid0) {
    const err8 = { instancePath, schemaPath: "#/oneOf", keyword: "oneOf", params: { passingSchemas: passing0 }, message: "must match exactly one schema in oneOf" };
    if (vErrors === null) {
      vErrors = [err8];
    } else {
      vErrors.push(err8);
    }
    errors++;
    validate52.errors = vErrors;
    return false;
  } else {
    errors = _errs0;
    if (vErrors !== null) {
      if (_errs0) {
        vErrors.length = _errs0;
      } else {
        vErrors = null;
      }
    }
  }
  validate52.errors = vErrors;
  evaluated0.props = props0;
  return errors === 0;
}
validate52.evaluated = { "dynamicProps": true, "dynamicItems": false };
function validate46(data, { instancePath = "", parentData, parentDataProperty, rootData = data, dynamicAnchors = {} } = {}) {
  let vErrors = null;
  let errors = 0;
  const evaluated0 = validate46.evaluated;
  if (evaluated0.dynamicProps) {
    evaluated0.props = void 0;
  }
  if (evaluated0.dynamicItems) {
    evaluated0.items = void 0;
  }
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.schemaVersion === void 0 && (missing0 = "schemaVersion") || data.snapshot === void 0 && (missing0 = "snapshot") || data.scope === void 0 && (missing0 = "scope") || data.work === void 0 && (missing0 = "work")) {
        validate46.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        if (data.schemaVersion !== void 0) {
          const _errs1 = errors;
          if ("agent_observability.dashboard_query.v1" !== data.schemaVersion) {
            validate46.errors = [{ instancePath: instancePath + "/schemaVersion", schemaPath: "#/properties/schemaVersion/const", keyword: "const", params: { allowedValue: "agent_observability.dashboard_query.v1" }, message: "must be equal to constant" }];
            return false;
          }
          var valid0 = _errs1 === errors;
        } else {
          var valid0 = true;
        }
        if (valid0) {
          if (data.snapshot !== void 0) {
            const _errs2 = errors;
            if (!validate47(data.snapshot, { instancePath: instancePath + "/snapshot", parentData: data, parentDataProperty: "snapshot", rootData, dynamicAnchors })) {
              vErrors = vErrors === null ? validate47.errors : vErrors.concat(validate47.errors);
              errors = vErrors.length;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.scope !== void 0) {
              const _errs3 = errors;
              if (!validate49(data.scope, { instancePath: instancePath + "/scope", parentData: data, parentDataProperty: "scope", rootData, dynamicAnchors })) {
                vErrors = vErrors === null ? validate49.errors : vErrors.concat(validate49.errors);
                errors = vErrors.length;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.work !== void 0) {
                const _errs4 = errors;
                if (!validate52(data.work, { instancePath: instancePath + "/work", parentData: data, parentDataProperty: "work", rootData, dynamicAnchors })) {
                  vErrors = vErrors === null ? validate52.errors : vErrors.concat(validate52.errors);
                  errors = vErrors.length;
                }
                var valid0 = _errs4 === errors;
              } else {
                var valid0 = true;
              }
            }
          }
        }
      }
    } else {
      validate46.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate46.errors = vErrors;
  return errors === 0;
}
validate46.evaluated = { "props": { "schemaVersion": true, "snapshot": true, "scope": true, "work": true }, "dynamicProps": false, "dynamicItems": false };
function validate45(data, { instancePath = "", parentData, parentDataProperty, rootData = data, dynamicAnchors = {} } = {}) {
  let vErrors = null;
  let errors = 0;
  const evaluated0 = validate45.evaluated;
  if (evaluated0.dynamicProps) {
    evaluated0.props = void 0;
  }
  if (evaluated0.dynamicItems) {
    evaluated0.items = void 0;
  }
  const _errs1 = errors;
  if (!validate46(data, { instancePath, parentData, parentDataProperty, rootData, dynamicAnchors })) {
    vErrors = vErrors === null ? validate46.errors : vErrors.concat(validate46.errors);
    errors = vErrors.length;
  }
  var valid0 = _errs1 === errors;
  if (valid0) {
    const _errs2 = errors;
    if (errors === _errs2) {
      if (data && typeof data == "object" && !Array.isArray(data)) {
        let missing0;
        if (data.kind === void 0 && (missing0 = "kind") || data.limits === void 0 && (missing0 = "limits")) {
          validate45.errors = [{ instancePath, schemaPath: "#/allOf/1/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
          return false;
        } else {
          if (data.kind !== void 0) {
            const _errs4 = errors;
            if ("bootstrap" !== data.kind) {
              validate45.errors = [{ instancePath: instancePath + "/kind", schemaPath: "#/allOf/1/properties/kind/const", keyword: "const", params: { allowedValue: "bootstrap" }, message: "must be equal to constant" }];
              return false;
            }
            var valid1 = _errs4 === errors;
          } else {
            var valid1 = true;
          }
          if (valid1) {
            if (data.limits !== void 0) {
              let data1 = data.limits;
              const _errs5 = errors;
              const _errs6 = errors;
              if (errors === _errs6) {
                if (data1 && typeof data1 == "object" && !Array.isArray(data1)) {
                  let missing1;
                  if (data1.traceRows === void 0 && (missing1 = "traceRows") || data1.spanRows === void 0 && (missing1 = "spanRows") || data1.timelineRows === void 0 && (missing1 = "timelineRows") || data1.facetValues === void 0 && (missing1 = "facetValues") || data1.requestBytes === void 0 && (missing1 = "requestBytes") || data1.responseBytes === void 0 && (missing1 = "responseBytes")) {
                    validate45.errors = [{ instancePath: instancePath + "/limits", schemaPath: "#/$defs/limits/required", keyword: "required", params: { missingProperty: missing1 }, message: "must have required property '" + missing1 + "'" }];
                    return false;
                  } else {
                    const _errs8 = errors;
                    for (const key0 in data1) {
                      if (!(key0 === "traceRows" || key0 === "spanRows" || key0 === "timelineRows" || key0 === "facetValues" || key0 === "requestBytes" || key0 === "responseBytes")) {
                        validate45.errors = [{ instancePath: instancePath + "/limits", schemaPath: "#/$defs/limits/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
                        return false;
                        break;
                      }
                    }
                    if (_errs8 === errors) {
                      if (data1.traceRows !== void 0) {
                        const _errs9 = errors;
                        if (100 !== data1.traceRows) {
                          validate45.errors = [{ instancePath: instancePath + "/limits/traceRows", schemaPath: "#/$defs/limits/properties/traceRows/const", keyword: "const", params: { allowedValue: 100 }, message: "must be equal to constant" }];
                          return false;
                        }
                        var valid3 = _errs9 === errors;
                      } else {
                        var valid3 = true;
                      }
                      if (valid3) {
                        if (data1.spanRows !== void 0) {
                          const _errs10 = errors;
                          if (200 !== data1.spanRows) {
                            validate45.errors = [{ instancePath: instancePath + "/limits/spanRows", schemaPath: "#/$defs/limits/properties/spanRows/const", keyword: "const", params: { allowedValue: 200 }, message: "must be equal to constant" }];
                            return false;
                          }
                          var valid3 = _errs10 === errors;
                        } else {
                          var valid3 = true;
                        }
                        if (valid3) {
                          if (data1.timelineRows !== void 0) {
                            const _errs11 = errors;
                            if (120 !== data1.timelineRows) {
                              validate45.errors = [{ instancePath: instancePath + "/limits/timelineRows", schemaPath: "#/$defs/limits/properties/timelineRows/const", keyword: "const", params: { allowedValue: 120 }, message: "must be equal to constant" }];
                              return false;
                            }
                            var valid3 = _errs11 === errors;
                          } else {
                            var valid3 = true;
                          }
                          if (valid3) {
                            if (data1.facetValues !== void 0) {
                              const _errs12 = errors;
                              if (500 !== data1.facetValues) {
                                validate45.errors = [{ instancePath: instancePath + "/limits/facetValues", schemaPath: "#/$defs/limits/properties/facetValues/const", keyword: "const", params: { allowedValue: 500 }, message: "must be equal to constant" }];
                                return false;
                              }
                              var valid3 = _errs12 === errors;
                            } else {
                              var valid3 = true;
                            }
                            if (valid3) {
                              if (data1.requestBytes !== void 0) {
                                const _errs13 = errors;
                                if (8192 !== data1.requestBytes) {
                                  validate45.errors = [{ instancePath: instancePath + "/limits/requestBytes", schemaPath: "#/$defs/limits/properties/requestBytes/const", keyword: "const", params: { allowedValue: 8192 }, message: "must be equal to constant" }];
                                  return false;
                                }
                                var valid3 = _errs13 === errors;
                              } else {
                                var valid3 = true;
                              }
                              if (valid3) {
                                if (data1.responseBytes !== void 0) {
                                  const _errs14 = errors;
                                  if (1048576 !== data1.responseBytes) {
                                    validate45.errors = [{ instancePath: instancePath + "/limits/responseBytes", schemaPath: "#/$defs/limits/properties/responseBytes/const", keyword: "const", params: { allowedValue: 1048576 }, message: "must be equal to constant" }];
                                    return false;
                                  }
                                  var valid3 = _errs14 === errors;
                                } else {
                                  var valid3 = true;
                                }
                              }
                            }
                          }
                        }
                      }
                    }
                  }
                } else {
                  validate45.errors = [{ instancePath: instancePath + "/limits", schemaPath: "#/$defs/limits/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
                  return false;
                }
              }
              var valid1 = _errs5 === errors;
            } else {
              var valid1 = true;
            }
          }
        }
      } else {
        validate45.errors = [{ instancePath, schemaPath: "#/allOf/1/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
        return false;
      }
    }
    var valid0 = _errs2 === errors;
  }
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      for (const key1 in data) {
        if (key1 !== "kind" && key1 !== "limits" && key1 !== "schemaVersion" && key1 !== "snapshot" && key1 !== "scope" && key1 !== "work") {
          validate45.errors = [{ instancePath, schemaPath: "#/unevaluatedProperties", keyword: "unevaluatedProperties", params: { unevaluatedProperty: key1 }, message: "must NOT have unevaluated properties" }];
          return false;
          break;
        }
      }
    } else {
      validate45.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate45.errors = vErrors;
  return errors === 0;
}
validate45.evaluated = { "props": true, "dynamicProps": false, "dynamicItems": false };
var schema80 = { "title": "DashboardTracesResponseV1", "type": "object", "allOf": [{ "$ref": "#/$defs/response_base" }, { "type": "object", "required": ["kind", "rows", "pagination"], "properties": { "kind": { "const": "traces" }, "rows": { "type": "array", "maxItems": 100, "items": { "$ref": "#/$defs/trace_row" } }, "pagination": { "$ref": "#/$defs/pagination" } } }], "unevaluatedProperties": false };
var schema81 = { "title": "DashboardTraceRowV1", "type": "object", "additionalProperties": false, "required": ["traceId", "repo", "spanCount", "errorCount", "startTimeUnixMs", "endTimeUnixMs", "availabilityReasons"], "properties": { "traceId": { "$ref": "#/$defs/projected_id" }, "repo": { "type": "string", "minLength": 1, "maxLength": 256 }, "spanCount": { "$ref": "#/$defs/safe_count" }, "errorCount": { "$ref": "#/$defs/safe_count" }, "startTimeUnixMs": { "type": "number", "minimum": 0 }, "endTimeUnixMs": { "oneOf": [{ "type": "number", "minimum": 0 }, { "type": "null" }] }, "availabilityReasons": { "$ref": "#/$defs/availability_reasons" } } };
var schema85 = { "type": "array", "maxItems": 9, "items": { "$ref": "#/$defs/availability_reason" }, "allOf": [{ "contains": { "type": "object", "properties": { "field": { "const": "repository" } }, "required": ["field"] }, "minContains": 0, "maxContains": 1 }, { "contains": { "type": "object", "properties": { "field": { "const": "session" } }, "required": ["field"] }, "minContains": 0, "maxContains": 1 }, { "contains": { "type": "object", "properties": { "field": { "const": "turn" } }, "required": ["field"] }, "minContains": 0, "maxContains": 1 }, { "contains": { "type": "object", "properties": { "field": { "const": "model" } }, "required": ["field"] }, "minContains": 0, "maxContains": 1 }, { "contains": { "type": "object", "properties": { "field": { "const": "tokens" } }, "required": ["field"] }, "minContains": 0, "maxContains": 1 }, { "contains": { "type": "object", "properties": { "field": { "const": "latency" } }, "required": ["field"] }, "minContains": 0, "maxContains": 1 }, { "contains": { "type": "object", "properties": { "field": { "const": "source_location" } }, "required": ["field"] }, "minContains": 0, "maxContains": 1 }, { "contains": { "type": "object", "properties": { "field": { "const": "request_content" } }, "required": ["field"] }, "minContains": 0, "maxContains": 1 }, { "contains": { "type": "object", "properties": { "field": { "const": "response_content" } }, "required": ["field"] }, "minContains": 0, "maxContains": 1 }] };
var schema86 = { "title": "DashboardAvailabilityReasonV1", "type": "object", "additionalProperties": false, "required": ["field", "state", "reason"], "properties": { "field": { "$ref": "#/$defs/availability_field" }, "state": { "$ref": "report-dto-v2.schema.json#/$defs/field_availability/properties/state" }, "reason": { "$ref": "report-dto-v2.schema.json#/$defs/field_availability/properties/reason" } }, "oneOf": [{ "type": "object", "required": ["state", "reason"], "properties": { "state": { "const": "source_unavailable" }, "reason": { "enum": ["source_not_provided", "not_evaluated", "partial_token_metrics", "historical_codex_source_not_lookup_eligible", "codex_notify_turn_correlation_unavailable", "ambiguous_trace_repository", "legacy_v1_report"] } } }, { "type": "object", "required": ["state", "reason"], "properties": { "state": { "const": "not_applicable" }, "reason": { "enum": ["span_kind_not_model_backed", "span_kind_has_no_latency", "span_kind_has_no_token_usage", "claude_private_lookup_not_supported", "cursor_private_lookup_not_supported", "codex_span_not_notify_derived", "agent_private_lookup_not_supported"] } } }, { "type": "object", "required": ["state", "reason"], "properties": { "state": { "const": "private_lookup" }, "reason": { "const": "local_opt_in_lookup_required" } } }, { "type": "object", "required": ["state", "reason"], "properties": { "state": { "const": "withheld" }, "reason": { "const": "withheld_by_privacy_policy" } } }] };
var schema87 = { "title": "DashboardAvailabilityFieldV1", "type": "string", "enum": ["repository", "session", "turn", "model", "tokens", "latency", "source_location", "request_content", "response_content"] };
var schema129 = { "enum": ["available", "source_unavailable", "withheld", "not_applicable", "private_lookup"] };
var schema130 = { "type": "string", "enum": ["reported_by_adapter", "derived_from_trace_context", "legacy_v1_report", "source_not_provided", "not_evaluated", "partial_token_metrics", "historical_codex_source_not_lookup_eligible", "codex_notify_turn_correlation_unavailable", "ambiguous_trace_repository", "span_kind_not_model_backed", "span_kind_has_no_latency", "span_kind_has_no_token_usage", "claude_private_lookup_not_supported", "cursor_private_lookup_not_supported", "codex_span_not_notify_derived", "agent_private_lookup_not_supported", "local_opt_in_lookup_required", "withheld_by_privacy_policy"] };
function validate66(data, { instancePath = "", parentData, parentDataProperty, rootData = data, dynamicAnchors = {} } = {}) {
  let vErrors = null;
  let errors = 0;
  const evaluated0 = validate66.evaluated;
  if (evaluated0.dynamicProps) {
    evaluated0.props = void 0;
  }
  if (evaluated0.dynamicItems) {
    evaluated0.items = void 0;
  }
  const _errs1 = errors;
  let valid0 = false;
  let passing0 = null;
  const _errs2 = errors;
  if (errors === _errs2) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.state === void 0 && (missing0 = "state") || data.reason === void 0 && (missing0 = "reason")) {
        const err0 = { instancePath, schemaPath: "#/oneOf/0/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" };
        if (vErrors === null) {
          vErrors = [err0];
        } else {
          vErrors.push(err0);
        }
        errors++;
      } else {
        if (data.state !== void 0) {
          const _errs4 = errors;
          if ("source_unavailable" !== data.state) {
            const err1 = { instancePath: instancePath + "/state", schemaPath: "#/oneOf/0/properties/state/const", keyword: "const", params: { allowedValue: "source_unavailable" }, message: "must be equal to constant" };
            if (vErrors === null) {
              vErrors = [err1];
            } else {
              vErrors.push(err1);
            }
            errors++;
          }
          var valid1 = _errs4 === errors;
        } else {
          var valid1 = true;
        }
        if (valid1) {
          if (data.reason !== void 0) {
            let data1 = data.reason;
            const _errs5 = errors;
            if (!(data1 === "source_not_provided" || data1 === "not_evaluated" || data1 === "partial_token_metrics" || data1 === "historical_codex_source_not_lookup_eligible" || data1 === "codex_notify_turn_correlation_unavailable" || data1 === "ambiguous_trace_repository" || data1 === "legacy_v1_report")) {
              const err2 = { instancePath: instancePath + "/reason", schemaPath: "#/oneOf/0/properties/reason/enum", keyword: "enum", params: { allowedValues: schema86.oneOf[0].properties.reason.enum }, message: "must be equal to one of the allowed values" };
              if (vErrors === null) {
                vErrors = [err2];
              } else {
                vErrors.push(err2);
              }
              errors++;
            }
            var valid1 = _errs5 === errors;
          } else {
            var valid1 = true;
          }
        }
      }
    } else {
      const err3 = { instancePath, schemaPath: "#/oneOf/0/type", keyword: "type", params: { type: "object" }, message: "must be object" };
      if (vErrors === null) {
        vErrors = [err3];
      } else {
        vErrors.push(err3);
      }
      errors++;
    }
  }
  var _valid0 = _errs2 === errors;
  if (_valid0) {
    valid0 = true;
    passing0 = 0;
    var props0 = {};
    props0.state = true;
    props0.reason = true;
  }
  const _errs6 = errors;
  if (errors === _errs6) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing1;
      if (data.state === void 0 && (missing1 = "state") || data.reason === void 0 && (missing1 = "reason")) {
        const err4 = { instancePath, schemaPath: "#/oneOf/1/required", keyword: "required", params: { missingProperty: missing1 }, message: "must have required property '" + missing1 + "'" };
        if (vErrors === null) {
          vErrors = [err4];
        } else {
          vErrors.push(err4);
        }
        errors++;
      } else {
        if (data.state !== void 0) {
          const _errs8 = errors;
          if ("not_applicable" !== data.state) {
            const err5 = { instancePath: instancePath + "/state", schemaPath: "#/oneOf/1/properties/state/const", keyword: "const", params: { allowedValue: "not_applicable" }, message: "must be equal to constant" };
            if (vErrors === null) {
              vErrors = [err5];
            } else {
              vErrors.push(err5);
            }
            errors++;
          }
          var valid2 = _errs8 === errors;
        } else {
          var valid2 = true;
        }
        if (valid2) {
          if (data.reason !== void 0) {
            let data3 = data.reason;
            const _errs9 = errors;
            if (!(data3 === "span_kind_not_model_backed" || data3 === "span_kind_has_no_latency" || data3 === "span_kind_has_no_token_usage" || data3 === "claude_private_lookup_not_supported" || data3 === "cursor_private_lookup_not_supported" || data3 === "codex_span_not_notify_derived" || data3 === "agent_private_lookup_not_supported")) {
              const err6 = { instancePath: instancePath + "/reason", schemaPath: "#/oneOf/1/properties/reason/enum", keyword: "enum", params: { allowedValues: schema86.oneOf[1].properties.reason.enum }, message: "must be equal to one of the allowed values" };
              if (vErrors === null) {
                vErrors = [err6];
              } else {
                vErrors.push(err6);
              }
              errors++;
            }
            var valid2 = _errs9 === errors;
          } else {
            var valid2 = true;
          }
        }
      }
    } else {
      const err7 = { instancePath, schemaPath: "#/oneOf/1/type", keyword: "type", params: { type: "object" }, message: "must be object" };
      if (vErrors === null) {
        vErrors = [err7];
      } else {
        vErrors.push(err7);
      }
      errors++;
    }
  }
  var _valid0 = _errs6 === errors;
  if (_valid0 && valid0) {
    valid0 = false;
    passing0 = [passing0, 1];
  } else {
    if (_valid0) {
      valid0 = true;
      passing0 = 1;
      if (props0 !== true) {
        props0 = props0 || {};
        props0.state = true;
        props0.reason = true;
      }
    }
    const _errs10 = errors;
    if (errors === _errs10) {
      if (data && typeof data == "object" && !Array.isArray(data)) {
        let missing2;
        if (data.state === void 0 && (missing2 = "state") || data.reason === void 0 && (missing2 = "reason")) {
          const err8 = { instancePath, schemaPath: "#/oneOf/2/required", keyword: "required", params: { missingProperty: missing2 }, message: "must have required property '" + missing2 + "'" };
          if (vErrors === null) {
            vErrors = [err8];
          } else {
            vErrors.push(err8);
          }
          errors++;
        } else {
          if (data.state !== void 0) {
            const _errs12 = errors;
            if ("private_lookup" !== data.state) {
              const err9 = { instancePath: instancePath + "/state", schemaPath: "#/oneOf/2/properties/state/const", keyword: "const", params: { allowedValue: "private_lookup" }, message: "must be equal to constant" };
              if (vErrors === null) {
                vErrors = [err9];
              } else {
                vErrors.push(err9);
              }
              errors++;
            }
            var valid3 = _errs12 === errors;
          } else {
            var valid3 = true;
          }
          if (valid3) {
            if (data.reason !== void 0) {
              const _errs13 = errors;
              if ("local_opt_in_lookup_required" !== data.reason) {
                const err10 = { instancePath: instancePath + "/reason", schemaPath: "#/oneOf/2/properties/reason/const", keyword: "const", params: { allowedValue: "local_opt_in_lookup_required" }, message: "must be equal to constant" };
                if (vErrors === null) {
                  vErrors = [err10];
                } else {
                  vErrors.push(err10);
                }
                errors++;
              }
              var valid3 = _errs13 === errors;
            } else {
              var valid3 = true;
            }
          }
        }
      } else {
        const err11 = { instancePath, schemaPath: "#/oneOf/2/type", keyword: "type", params: { type: "object" }, message: "must be object" };
        if (vErrors === null) {
          vErrors = [err11];
        } else {
          vErrors.push(err11);
        }
        errors++;
      }
    }
    var _valid0 = _errs10 === errors;
    if (_valid0 && valid0) {
      valid0 = false;
      passing0 = [passing0, 2];
    } else {
      if (_valid0) {
        valid0 = true;
        passing0 = 2;
        if (props0 !== true) {
          props0 = props0 || {};
          props0.state = true;
          props0.reason = true;
        }
      }
      const _errs14 = errors;
      if (errors === _errs14) {
        if (data && typeof data == "object" && !Array.isArray(data)) {
          let missing3;
          if (data.state === void 0 && (missing3 = "state") || data.reason === void 0 && (missing3 = "reason")) {
            const err12 = { instancePath, schemaPath: "#/oneOf/3/required", keyword: "required", params: { missingProperty: missing3 }, message: "must have required property '" + missing3 + "'" };
            if (vErrors === null) {
              vErrors = [err12];
            } else {
              vErrors.push(err12);
            }
            errors++;
          } else {
            if (data.state !== void 0) {
              const _errs16 = errors;
              if ("withheld" !== data.state) {
                const err13 = { instancePath: instancePath + "/state", schemaPath: "#/oneOf/3/properties/state/const", keyword: "const", params: { allowedValue: "withheld" }, message: "must be equal to constant" };
                if (vErrors === null) {
                  vErrors = [err13];
                } else {
                  vErrors.push(err13);
                }
                errors++;
              }
              var valid4 = _errs16 === errors;
            } else {
              var valid4 = true;
            }
            if (valid4) {
              if (data.reason !== void 0) {
                const _errs17 = errors;
                if ("withheld_by_privacy_policy" !== data.reason) {
                  const err14 = { instancePath: instancePath + "/reason", schemaPath: "#/oneOf/3/properties/reason/const", keyword: "const", params: { allowedValue: "withheld_by_privacy_policy" }, message: "must be equal to constant" };
                  if (vErrors === null) {
                    vErrors = [err14];
                  } else {
                    vErrors.push(err14);
                  }
                  errors++;
                }
                var valid4 = _errs17 === errors;
              } else {
                var valid4 = true;
              }
            }
          }
        } else {
          const err15 = { instancePath, schemaPath: "#/oneOf/3/type", keyword: "type", params: { type: "object" }, message: "must be object" };
          if (vErrors === null) {
            vErrors = [err15];
          } else {
            vErrors.push(err15);
          }
          errors++;
        }
      }
      var _valid0 = _errs14 === errors;
      if (_valid0 && valid0) {
        valid0 = false;
        passing0 = [passing0, 3];
      } else {
        if (_valid0) {
          valid0 = true;
          passing0 = 3;
          if (props0 !== true) {
            props0 = props0 || {};
            props0.state = true;
            props0.reason = true;
          }
        }
      }
    }
  }
  if (!valid0) {
    const err16 = { instancePath, schemaPath: "#/oneOf", keyword: "oneOf", params: { passingSchemas: passing0 }, message: "must match exactly one schema in oneOf" };
    if (vErrors === null) {
      vErrors = [err16];
    } else {
      vErrors.push(err16);
    }
    errors++;
    validate66.errors = vErrors;
    return false;
  } else {
    errors = _errs1;
    if (vErrors !== null) {
      if (_errs1) {
        vErrors.length = _errs1;
      } else {
        vErrors = null;
      }
    }
  }
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing4;
      if (data.field === void 0 && (missing4 = "field") || data.state === void 0 && (missing4 = "state") || data.reason === void 0 && (missing4 = "reason")) {
        validate66.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing4 }, message: "must have required property '" + missing4 + "'" }];
        return false;
      } else {
        const _errs18 = errors;
        for (const key0 in data) {
          if (!(key0 === "field" || key0 === "state" || key0 === "reason")) {
            validate66.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs18 === errors) {
          if (data.field !== void 0) {
            let data8 = data.field;
            const _errs19 = errors;
            if (typeof data8 !== "string") {
              validate66.errors = [{ instancePath: instancePath + "/field", schemaPath: "#/$defs/availability_field/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
              return false;
            }
            if (!(data8 === "repository" || data8 === "session" || data8 === "turn" || data8 === "model" || data8 === "tokens" || data8 === "latency" || data8 === "source_location" || data8 === "request_content" || data8 === "response_content")) {
              validate66.errors = [{ instancePath: instancePath + "/field", schemaPath: "#/$defs/availability_field/enum", keyword: "enum", params: { allowedValues: schema87.enum }, message: "must be equal to one of the allowed values" }];
              return false;
            }
            var valid5 = _errs19 === errors;
          } else {
            var valid5 = true;
          }
          if (valid5) {
            if (data.state !== void 0) {
              let data9 = data.state;
              const _errs22 = errors;
              if (!(data9 === "available" || data9 === "source_unavailable" || data9 === "withheld" || data9 === "not_applicable" || data9 === "private_lookup")) {
                validate66.errors = [{ instancePath: instancePath + "/state", schemaPath: "report-dto-v2.schema.json#/$defs/field_availability/properties/state/enum", keyword: "enum", params: { allowedValues: schema129.enum }, message: "must be equal to one of the allowed values" }];
                return false;
              }
              var valid5 = _errs22 === errors;
            } else {
              var valid5 = true;
            }
            if (valid5) {
              if (data.reason !== void 0) {
                let data10 = data.reason;
                const _errs24 = errors;
                if (typeof data10 !== "string") {
                  validate66.errors = [{ instancePath: instancePath + "/reason", schemaPath: "report-dto-v2.schema.json#/$defs/field_availability/properties/reason/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                  return false;
                }
                if (!(data10 === "reported_by_adapter" || data10 === "derived_from_trace_context" || data10 === "legacy_v1_report" || data10 === "source_not_provided" || data10 === "not_evaluated" || data10 === "partial_token_metrics" || data10 === "historical_codex_source_not_lookup_eligible" || data10 === "codex_notify_turn_correlation_unavailable" || data10 === "ambiguous_trace_repository" || data10 === "span_kind_not_model_backed" || data10 === "span_kind_has_no_latency" || data10 === "span_kind_has_no_token_usage" || data10 === "claude_private_lookup_not_supported" || data10 === "cursor_private_lookup_not_supported" || data10 === "codex_span_not_notify_derived" || data10 === "agent_private_lookup_not_supported" || data10 === "local_opt_in_lookup_required" || data10 === "withheld_by_privacy_policy")) {
                  validate66.errors = [{ instancePath: instancePath + "/reason", schemaPath: "report-dto-v2.schema.json#/$defs/field_availability/properties/reason/enum", keyword: "enum", params: { allowedValues: schema130.enum }, message: "must be equal to one of the allowed values" }];
                  return false;
                }
                var valid5 = _errs24 === errors;
              } else {
                var valid5 = true;
              }
            }
          }
        }
      }
    } else {
      validate66.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate66.errors = vErrors;
  return errors === 0;
}
validate66.evaluated = { "props": true, "dynamicProps": false, "dynamicItems": false };
function validate65(data, { instancePath = "", parentData, parentDataProperty, rootData = data, dynamicAnchors = {} } = {}) {
  let vErrors = null;
  let errors = 0;
  const evaluated0 = validate65.evaluated;
  if (evaluated0.dynamicProps) {
    evaluated0.props = void 0;
  }
  if (evaluated0.dynamicItems) {
    evaluated0.items = void 0;
  }
  const _errs1 = errors;
  if (Array.isArray(data)) {
    const _errs2 = errors;
    const len0 = data.length;
    let valid1 = true;
    if (data.length > 0) {
      let count0 = 0;
      for (let i0 = 0; i0 < len0; i0++) {
        let data0 = data[i0];
        const _errs3 = errors;
        if (errors === _errs3) {
          if (data0 && typeof data0 == "object" && !Array.isArray(data0)) {
            let missing0;
            if (data0.field === void 0 && (missing0 = "field")) {
              const err0 = { instancePath: instancePath + "/" + i0, schemaPath: "#/allOf/0/contains/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" };
              if (vErrors === null) {
                vErrors = [err0];
              } else {
                vErrors.push(err0);
              }
              errors++;
            } else {
              if (data0.field !== void 0) {
                if ("repository" !== data0.field) {
                  const err1 = { instancePath: instancePath + "/" + i0 + "/field", schemaPath: "#/allOf/0/contains/properties/field/const", keyword: "const", params: { allowedValue: "repository" }, message: "must be equal to constant" };
                  if (vErrors === null) {
                    vErrors = [err1];
                  } else {
                    vErrors.push(err1);
                  }
                  errors++;
                }
              }
            }
          } else {
            const err2 = { instancePath: instancePath + "/" + i0, schemaPath: "#/allOf/0/contains/type", keyword: "type", params: { type: "object" }, message: "must be object" };
            if (vErrors === null) {
              vErrors = [err2];
            } else {
              vErrors.push(err2);
            }
            errors++;
          }
        }
        var _valid0 = _errs3 === errors;
        if (_valid0) {
          count0++;
          if (count0 > 1) {
            valid1 = false;
            break;
          }
          if (count0 >= 0) {
            valid1 = true;
          }
        }
      }
    }
    if (!valid1) {
      validate65.errors = [{ instancePath, schemaPath: "#/allOf/0/contains", keyword: "contains", params: { minContains: 0, maxContains: 1 }, message: "must contain at least 0 and no more than 1 valid item(s)" }];
      return false;
    } else {
      errors = _errs2;
      if (vErrors !== null) {
        if (_errs2) {
          vErrors.length = _errs2;
        } else {
          vErrors = null;
        }
      }
    }
  }
  var valid0 = _errs1 === errors;
  if (valid0) {
    const _errs6 = errors;
    if (Array.isArray(data)) {
      const _errs7 = errors;
      const len1 = data.length;
      let valid3 = true;
      if (data.length > 0) {
        let count1 = 0;
        for (let i1 = 0; i1 < len1; i1++) {
          let data2 = data[i1];
          const _errs8 = errors;
          if (errors === _errs8) {
            if (data2 && typeof data2 == "object" && !Array.isArray(data2)) {
              let missing1;
              if (data2.field === void 0 && (missing1 = "field")) {
                const err3 = { instancePath: instancePath + "/" + i1, schemaPath: "#/allOf/1/contains/required", keyword: "required", params: { missingProperty: missing1 }, message: "must have required property '" + missing1 + "'" };
                if (vErrors === null) {
                  vErrors = [err3];
                } else {
                  vErrors.push(err3);
                }
                errors++;
              } else {
                if (data2.field !== void 0) {
                  if ("session" !== data2.field) {
                    const err4 = { instancePath: instancePath + "/" + i1 + "/field", schemaPath: "#/allOf/1/contains/properties/field/const", keyword: "const", params: { allowedValue: "session" }, message: "must be equal to constant" };
                    if (vErrors === null) {
                      vErrors = [err4];
                    } else {
                      vErrors.push(err4);
                    }
                    errors++;
                  }
                }
              }
            } else {
              const err5 = { instancePath: instancePath + "/" + i1, schemaPath: "#/allOf/1/contains/type", keyword: "type", params: { type: "object" }, message: "must be object" };
              if (vErrors === null) {
                vErrors = [err5];
              } else {
                vErrors.push(err5);
              }
              errors++;
            }
          }
          var _valid1 = _errs8 === errors;
          if (_valid1) {
            count1++;
            if (count1 > 1) {
              valid3 = false;
              break;
            }
            if (count1 >= 0) {
              valid3 = true;
            }
          }
        }
      }
      if (!valid3) {
        validate65.errors = [{ instancePath, schemaPath: "#/allOf/1/contains", keyword: "contains", params: { minContains: 0, maxContains: 1 }, message: "must contain at least 0 and no more than 1 valid item(s)" }];
        return false;
      } else {
        errors = _errs7;
        if (vErrors !== null) {
          if (_errs7) {
            vErrors.length = _errs7;
          } else {
            vErrors = null;
          }
        }
      }
    }
    var valid0 = _errs6 === errors;
    if (valid0) {
      const _errs11 = errors;
      if (Array.isArray(data)) {
        const _errs12 = errors;
        const len2 = data.length;
        let valid5 = true;
        if (data.length > 0) {
          let count2 = 0;
          for (let i2 = 0; i2 < len2; i2++) {
            let data4 = data[i2];
            const _errs13 = errors;
            if (errors === _errs13) {
              if (data4 && typeof data4 == "object" && !Array.isArray(data4)) {
                let missing2;
                if (data4.field === void 0 && (missing2 = "field")) {
                  const err6 = { instancePath: instancePath + "/" + i2, schemaPath: "#/allOf/2/contains/required", keyword: "required", params: { missingProperty: missing2 }, message: "must have required property '" + missing2 + "'" };
                  if (vErrors === null) {
                    vErrors = [err6];
                  } else {
                    vErrors.push(err6);
                  }
                  errors++;
                } else {
                  if (data4.field !== void 0) {
                    if ("turn" !== data4.field) {
                      const err7 = { instancePath: instancePath + "/" + i2 + "/field", schemaPath: "#/allOf/2/contains/properties/field/const", keyword: "const", params: { allowedValue: "turn" }, message: "must be equal to constant" };
                      if (vErrors === null) {
                        vErrors = [err7];
                      } else {
                        vErrors.push(err7);
                      }
                      errors++;
                    }
                  }
                }
              } else {
                const err8 = { instancePath: instancePath + "/" + i2, schemaPath: "#/allOf/2/contains/type", keyword: "type", params: { type: "object" }, message: "must be object" };
                if (vErrors === null) {
                  vErrors = [err8];
                } else {
                  vErrors.push(err8);
                }
                errors++;
              }
            }
            var _valid2 = _errs13 === errors;
            if (_valid2) {
              count2++;
              if (count2 > 1) {
                valid5 = false;
                break;
              }
              if (count2 >= 0) {
                valid5 = true;
              }
            }
          }
        }
        if (!valid5) {
          validate65.errors = [{ instancePath, schemaPath: "#/allOf/2/contains", keyword: "contains", params: { minContains: 0, maxContains: 1 }, message: "must contain at least 0 and no more than 1 valid item(s)" }];
          return false;
        } else {
          errors = _errs12;
          if (vErrors !== null) {
            if (_errs12) {
              vErrors.length = _errs12;
            } else {
              vErrors = null;
            }
          }
        }
      }
      var valid0 = _errs11 === errors;
      if (valid0) {
        const _errs16 = errors;
        if (Array.isArray(data)) {
          const _errs17 = errors;
          const len3 = data.length;
          let valid7 = true;
          if (data.length > 0) {
            let count3 = 0;
            for (let i3 = 0; i3 < len3; i3++) {
              let data6 = data[i3];
              const _errs18 = errors;
              if (errors === _errs18) {
                if (data6 && typeof data6 == "object" && !Array.isArray(data6)) {
                  let missing3;
                  if (data6.field === void 0 && (missing3 = "field")) {
                    const err9 = { instancePath: instancePath + "/" + i3, schemaPath: "#/allOf/3/contains/required", keyword: "required", params: { missingProperty: missing3 }, message: "must have required property '" + missing3 + "'" };
                    if (vErrors === null) {
                      vErrors = [err9];
                    } else {
                      vErrors.push(err9);
                    }
                    errors++;
                  } else {
                    if (data6.field !== void 0) {
                      if ("model" !== data6.field) {
                        const err10 = { instancePath: instancePath + "/" + i3 + "/field", schemaPath: "#/allOf/3/contains/properties/field/const", keyword: "const", params: { allowedValue: "model" }, message: "must be equal to constant" };
                        if (vErrors === null) {
                          vErrors = [err10];
                        } else {
                          vErrors.push(err10);
                        }
                        errors++;
                      }
                    }
                  }
                } else {
                  const err11 = { instancePath: instancePath + "/" + i3, schemaPath: "#/allOf/3/contains/type", keyword: "type", params: { type: "object" }, message: "must be object" };
                  if (vErrors === null) {
                    vErrors = [err11];
                  } else {
                    vErrors.push(err11);
                  }
                  errors++;
                }
              }
              var _valid3 = _errs18 === errors;
              if (_valid3) {
                count3++;
                if (count3 > 1) {
                  valid7 = false;
                  break;
                }
                if (count3 >= 0) {
                  valid7 = true;
                }
              }
            }
          }
          if (!valid7) {
            validate65.errors = [{ instancePath, schemaPath: "#/allOf/3/contains", keyword: "contains", params: { minContains: 0, maxContains: 1 }, message: "must contain at least 0 and no more than 1 valid item(s)" }];
            return false;
          } else {
            errors = _errs17;
            if (vErrors !== null) {
              if (_errs17) {
                vErrors.length = _errs17;
              } else {
                vErrors = null;
              }
            }
          }
        }
        var valid0 = _errs16 === errors;
        if (valid0) {
          const _errs21 = errors;
          if (Array.isArray(data)) {
            const _errs22 = errors;
            const len4 = data.length;
            let valid9 = true;
            if (data.length > 0) {
              let count4 = 0;
              for (let i4 = 0; i4 < len4; i4++) {
                let data8 = data[i4];
                const _errs23 = errors;
                if (errors === _errs23) {
                  if (data8 && typeof data8 == "object" && !Array.isArray(data8)) {
                    let missing4;
                    if (data8.field === void 0 && (missing4 = "field")) {
                      const err12 = { instancePath: instancePath + "/" + i4, schemaPath: "#/allOf/4/contains/required", keyword: "required", params: { missingProperty: missing4 }, message: "must have required property '" + missing4 + "'" };
                      if (vErrors === null) {
                        vErrors = [err12];
                      } else {
                        vErrors.push(err12);
                      }
                      errors++;
                    } else {
                      if (data8.field !== void 0) {
                        if ("tokens" !== data8.field) {
                          const err13 = { instancePath: instancePath + "/" + i4 + "/field", schemaPath: "#/allOf/4/contains/properties/field/const", keyword: "const", params: { allowedValue: "tokens" }, message: "must be equal to constant" };
                          if (vErrors === null) {
                            vErrors = [err13];
                          } else {
                            vErrors.push(err13);
                          }
                          errors++;
                        }
                      }
                    }
                  } else {
                    const err14 = { instancePath: instancePath + "/" + i4, schemaPath: "#/allOf/4/contains/type", keyword: "type", params: { type: "object" }, message: "must be object" };
                    if (vErrors === null) {
                      vErrors = [err14];
                    } else {
                      vErrors.push(err14);
                    }
                    errors++;
                  }
                }
                var _valid4 = _errs23 === errors;
                if (_valid4) {
                  count4++;
                  if (count4 > 1) {
                    valid9 = false;
                    break;
                  }
                  if (count4 >= 0) {
                    valid9 = true;
                  }
                }
              }
            }
            if (!valid9) {
              validate65.errors = [{ instancePath, schemaPath: "#/allOf/4/contains", keyword: "contains", params: { minContains: 0, maxContains: 1 }, message: "must contain at least 0 and no more than 1 valid item(s)" }];
              return false;
            } else {
              errors = _errs22;
              if (vErrors !== null) {
                if (_errs22) {
                  vErrors.length = _errs22;
                } else {
                  vErrors = null;
                }
              }
            }
          }
          var valid0 = _errs21 === errors;
          if (valid0) {
            const _errs26 = errors;
            if (Array.isArray(data)) {
              const _errs27 = errors;
              const len5 = data.length;
              let valid11 = true;
              if (data.length > 0) {
                let count5 = 0;
                for (let i5 = 0; i5 < len5; i5++) {
                  let data10 = data[i5];
                  const _errs28 = errors;
                  if (errors === _errs28) {
                    if (data10 && typeof data10 == "object" && !Array.isArray(data10)) {
                      let missing5;
                      if (data10.field === void 0 && (missing5 = "field")) {
                        const err15 = { instancePath: instancePath + "/" + i5, schemaPath: "#/allOf/5/contains/required", keyword: "required", params: { missingProperty: missing5 }, message: "must have required property '" + missing5 + "'" };
                        if (vErrors === null) {
                          vErrors = [err15];
                        } else {
                          vErrors.push(err15);
                        }
                        errors++;
                      } else {
                        if (data10.field !== void 0) {
                          if ("latency" !== data10.field) {
                            const err16 = { instancePath: instancePath + "/" + i5 + "/field", schemaPath: "#/allOf/5/contains/properties/field/const", keyword: "const", params: { allowedValue: "latency" }, message: "must be equal to constant" };
                            if (vErrors === null) {
                              vErrors = [err16];
                            } else {
                              vErrors.push(err16);
                            }
                            errors++;
                          }
                        }
                      }
                    } else {
                      const err17 = { instancePath: instancePath + "/" + i5, schemaPath: "#/allOf/5/contains/type", keyword: "type", params: { type: "object" }, message: "must be object" };
                      if (vErrors === null) {
                        vErrors = [err17];
                      } else {
                        vErrors.push(err17);
                      }
                      errors++;
                    }
                  }
                  var _valid5 = _errs28 === errors;
                  if (_valid5) {
                    count5++;
                    if (count5 > 1) {
                      valid11 = false;
                      break;
                    }
                    if (count5 >= 0) {
                      valid11 = true;
                    }
                  }
                }
              }
              if (!valid11) {
                validate65.errors = [{ instancePath, schemaPath: "#/allOf/5/contains", keyword: "contains", params: { minContains: 0, maxContains: 1 }, message: "must contain at least 0 and no more than 1 valid item(s)" }];
                return false;
              } else {
                errors = _errs27;
                if (vErrors !== null) {
                  if (_errs27) {
                    vErrors.length = _errs27;
                  } else {
                    vErrors = null;
                  }
                }
              }
            }
            var valid0 = _errs26 === errors;
            if (valid0) {
              const _errs31 = errors;
              if (Array.isArray(data)) {
                const _errs32 = errors;
                const len6 = data.length;
                let valid13 = true;
                if (data.length > 0) {
                  let count6 = 0;
                  for (let i6 = 0; i6 < len6; i6++) {
                    let data12 = data[i6];
                    const _errs33 = errors;
                    if (errors === _errs33) {
                      if (data12 && typeof data12 == "object" && !Array.isArray(data12)) {
                        let missing6;
                        if (data12.field === void 0 && (missing6 = "field")) {
                          const err18 = { instancePath: instancePath + "/" + i6, schemaPath: "#/allOf/6/contains/required", keyword: "required", params: { missingProperty: missing6 }, message: "must have required property '" + missing6 + "'" };
                          if (vErrors === null) {
                            vErrors = [err18];
                          } else {
                            vErrors.push(err18);
                          }
                          errors++;
                        } else {
                          if (data12.field !== void 0) {
                            if ("source_location" !== data12.field) {
                              const err19 = { instancePath: instancePath + "/" + i6 + "/field", schemaPath: "#/allOf/6/contains/properties/field/const", keyword: "const", params: { allowedValue: "source_location" }, message: "must be equal to constant" };
                              if (vErrors === null) {
                                vErrors = [err19];
                              } else {
                                vErrors.push(err19);
                              }
                              errors++;
                            }
                          }
                        }
                      } else {
                        const err20 = { instancePath: instancePath + "/" + i6, schemaPath: "#/allOf/6/contains/type", keyword: "type", params: { type: "object" }, message: "must be object" };
                        if (vErrors === null) {
                          vErrors = [err20];
                        } else {
                          vErrors.push(err20);
                        }
                        errors++;
                      }
                    }
                    var _valid6 = _errs33 === errors;
                    if (_valid6) {
                      count6++;
                      if (count6 > 1) {
                        valid13 = false;
                        break;
                      }
                      if (count6 >= 0) {
                        valid13 = true;
                      }
                    }
                  }
                }
                if (!valid13) {
                  validate65.errors = [{ instancePath, schemaPath: "#/allOf/6/contains", keyword: "contains", params: { minContains: 0, maxContains: 1 }, message: "must contain at least 0 and no more than 1 valid item(s)" }];
                  return false;
                } else {
                  errors = _errs32;
                  if (vErrors !== null) {
                    if (_errs32) {
                      vErrors.length = _errs32;
                    } else {
                      vErrors = null;
                    }
                  }
                }
              }
              var valid0 = _errs31 === errors;
              if (valid0) {
                const _errs36 = errors;
                if (Array.isArray(data)) {
                  const _errs37 = errors;
                  const len7 = data.length;
                  let valid15 = true;
                  if (data.length > 0) {
                    let count7 = 0;
                    for (let i7 = 0; i7 < len7; i7++) {
                      let data14 = data[i7];
                      const _errs38 = errors;
                      if (errors === _errs38) {
                        if (data14 && typeof data14 == "object" && !Array.isArray(data14)) {
                          let missing7;
                          if (data14.field === void 0 && (missing7 = "field")) {
                            const err21 = { instancePath: instancePath + "/" + i7, schemaPath: "#/allOf/7/contains/required", keyword: "required", params: { missingProperty: missing7 }, message: "must have required property '" + missing7 + "'" };
                            if (vErrors === null) {
                              vErrors = [err21];
                            } else {
                              vErrors.push(err21);
                            }
                            errors++;
                          } else {
                            if (data14.field !== void 0) {
                              if ("request_content" !== data14.field) {
                                const err22 = { instancePath: instancePath + "/" + i7 + "/field", schemaPath: "#/allOf/7/contains/properties/field/const", keyword: "const", params: { allowedValue: "request_content" }, message: "must be equal to constant" };
                                if (vErrors === null) {
                                  vErrors = [err22];
                                } else {
                                  vErrors.push(err22);
                                }
                                errors++;
                              }
                            }
                          }
                        } else {
                          const err23 = { instancePath: instancePath + "/" + i7, schemaPath: "#/allOf/7/contains/type", keyword: "type", params: { type: "object" }, message: "must be object" };
                          if (vErrors === null) {
                            vErrors = [err23];
                          } else {
                            vErrors.push(err23);
                          }
                          errors++;
                        }
                      }
                      var _valid7 = _errs38 === errors;
                      if (_valid7) {
                        count7++;
                        if (count7 > 1) {
                          valid15 = false;
                          break;
                        }
                        if (count7 >= 0) {
                          valid15 = true;
                        }
                      }
                    }
                  }
                  if (!valid15) {
                    validate65.errors = [{ instancePath, schemaPath: "#/allOf/7/contains", keyword: "contains", params: { minContains: 0, maxContains: 1 }, message: "must contain at least 0 and no more than 1 valid item(s)" }];
                    return false;
                  } else {
                    errors = _errs37;
                    if (vErrors !== null) {
                      if (_errs37) {
                        vErrors.length = _errs37;
                      } else {
                        vErrors = null;
                      }
                    }
                  }
                }
                var valid0 = _errs36 === errors;
                if (valid0) {
                  const _errs41 = errors;
                  if (Array.isArray(data)) {
                    const _errs42 = errors;
                    const len8 = data.length;
                    let valid17 = true;
                    if (data.length > 0) {
                      let count8 = 0;
                      for (let i8 = 0; i8 < len8; i8++) {
                        let data16 = data[i8];
                        const _errs43 = errors;
                        if (errors === _errs43) {
                          if (data16 && typeof data16 == "object" && !Array.isArray(data16)) {
                            let missing8;
                            if (data16.field === void 0 && (missing8 = "field")) {
                              const err24 = { instancePath: instancePath + "/" + i8, schemaPath: "#/allOf/8/contains/required", keyword: "required", params: { missingProperty: missing8 }, message: "must have required property '" + missing8 + "'" };
                              if (vErrors === null) {
                                vErrors = [err24];
                              } else {
                                vErrors.push(err24);
                              }
                              errors++;
                            } else {
                              if (data16.field !== void 0) {
                                if ("response_content" !== data16.field) {
                                  const err25 = { instancePath: instancePath + "/" + i8 + "/field", schemaPath: "#/allOf/8/contains/properties/field/const", keyword: "const", params: { allowedValue: "response_content" }, message: "must be equal to constant" };
                                  if (vErrors === null) {
                                    vErrors = [err25];
                                  } else {
                                    vErrors.push(err25);
                                  }
                                  errors++;
                                }
                              }
                            }
                          } else {
                            const err26 = { instancePath: instancePath + "/" + i8, schemaPath: "#/allOf/8/contains/type", keyword: "type", params: { type: "object" }, message: "must be object" };
                            if (vErrors === null) {
                              vErrors = [err26];
                            } else {
                              vErrors.push(err26);
                            }
                            errors++;
                          }
                        }
                        var _valid8 = _errs43 === errors;
                        if (_valid8) {
                          count8++;
                          if (count8 > 1) {
                            valid17 = false;
                            break;
                          }
                          if (count8 >= 0) {
                            valid17 = true;
                          }
                        }
                      }
                    }
                    if (!valid17) {
                      validate65.errors = [{ instancePath, schemaPath: "#/allOf/8/contains", keyword: "contains", params: { minContains: 0, maxContains: 1 }, message: "must contain at least 0 and no more than 1 valid item(s)" }];
                      return false;
                    } else {
                      errors = _errs42;
                      if (vErrors !== null) {
                        if (_errs42) {
                          vErrors.length = _errs42;
                        } else {
                          vErrors = null;
                        }
                      }
                    }
                  }
                  var valid0 = _errs41 === errors;
                }
              }
            }
          }
        }
      }
    }
  }
  if (errors === 0) {
    if (Array.isArray(data)) {
      if (data.length > 9) {
        validate65.errors = [{ instancePath, schemaPath: "#/maxItems", keyword: "maxItems", params: { limit: 9 }, message: "must NOT have more than 9 items" }];
        return false;
      } else {
        var valid19 = true;
        const len9 = data.length;
        for (let i9 = 0; i9 < len9; i9++) {
          const _errs46 = errors;
          if (!validate66(data[i9], { instancePath: instancePath + "/" + i9, parentData: data, parentDataProperty: i9, rootData, dynamicAnchors })) {
            vErrors = vErrors === null ? validate66.errors : vErrors.concat(validate66.errors);
            errors = vErrors.length;
          }
          var valid19 = _errs46 === errors;
          if (!valid19) {
            break;
          }
        }
      }
    } else {
      validate65.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
      return false;
    }
  }
  validate65.errors = vErrors;
  return errors === 0;
}
validate65.evaluated = { "items": true, "dynamicProps": false, "dynamicItems": false };
function validate64(data, { instancePath = "", parentData, parentDataProperty, rootData = data, dynamicAnchors = {} } = {}) {
  let vErrors = null;
  let errors = 0;
  const evaluated0 = validate64.evaluated;
  if (evaluated0.dynamicProps) {
    evaluated0.props = void 0;
  }
  if (evaluated0.dynamicItems) {
    evaluated0.items = void 0;
  }
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.traceId === void 0 && (missing0 = "traceId") || data.repo === void 0 && (missing0 = "repo") || data.spanCount === void 0 && (missing0 = "spanCount") || data.errorCount === void 0 && (missing0 = "errorCount") || data.startTimeUnixMs === void 0 && (missing0 = "startTimeUnixMs") || data.endTimeUnixMs === void 0 && (missing0 = "endTimeUnixMs") || data.availabilityReasons === void 0 && (missing0 = "availabilityReasons")) {
        validate64.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "traceId" || key0 === "repo" || key0 === "spanCount" || key0 === "errorCount" || key0 === "startTimeUnixMs" || key0 === "endTimeUnixMs" || key0 === "availabilityReasons")) {
            validate64.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.traceId !== void 0) {
            let data0 = data.traceId;
            const _errs2 = errors;
            const _errs3 = errors;
            if (errors === _errs3) {
              if (typeof data0 === "string") {
                if (!pattern6.test(data0)) {
                  validate64.errors = [{ instancePath: instancePath + "/traceId", schemaPath: "#/$defs/projected_id/pattern", keyword: "pattern", params: { pattern: "^id:sha256:[0-9a-f]{64}$" }, message: 'must match pattern "^id:sha256:[0-9a-f]{64}$"' }];
                  return false;
                }
              } else {
                validate64.errors = [{ instancePath: instancePath + "/traceId", schemaPath: "#/$defs/projected_id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                return false;
              }
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.repo !== void 0) {
              let data1 = data.repo;
              const _errs5 = errors;
              if (errors === _errs5) {
                if (typeof data1 === "string") {
                  if (func1(data1) > 256) {
                    validate64.errors = [{ instancePath: instancePath + "/repo", schemaPath: "#/properties/repo/maxLength", keyword: "maxLength", params: { limit: 256 }, message: "must NOT have more than 256 characters" }];
                    return false;
                  } else {
                    if (func1(data1) < 1) {
                      validate64.errors = [{ instancePath: instancePath + "/repo", schemaPath: "#/properties/repo/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                      return false;
                    }
                  }
                } else {
                  validate64.errors = [{ instancePath: instancePath + "/repo", schemaPath: "#/properties/repo/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                  return false;
                }
              }
              var valid0 = _errs5 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.spanCount !== void 0) {
                let data2 = data.spanCount;
                const _errs7 = errors;
                const _errs8 = errors;
                if (!(typeof data2 == "number" && (!(data2 % 1) && !isNaN(data2)) && isFinite(data2))) {
                  validate64.errors = [{ instancePath: instancePath + "/spanCount", schemaPath: "#/$defs/safe_count/type", keyword: "type", params: { type: "integer" }, message: "must be integer" }];
                  return false;
                }
                if (errors === _errs8) {
                  if (typeof data2 == "number" && isFinite(data2)) {
                    if (data2 > 9007199254740991 || isNaN(data2)) {
                      validate64.errors = [{ instancePath: instancePath + "/spanCount", schemaPath: "#/$defs/safe_count/maximum", keyword: "maximum", params: { comparison: "<=", limit: 9007199254740991 }, message: "must be <= 9007199254740991" }];
                      return false;
                    } else {
                      if (data2 < 0 || isNaN(data2)) {
                        validate64.errors = [{ instancePath: instancePath + "/spanCount", schemaPath: "#/$defs/safe_count/minimum", keyword: "minimum", params: { comparison: ">=", limit: 0 }, message: "must be >= 0" }];
                        return false;
                      }
                    }
                  }
                }
                var valid0 = _errs7 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.errorCount !== void 0) {
                  let data3 = data.errorCount;
                  const _errs10 = errors;
                  const _errs11 = errors;
                  if (!(typeof data3 == "number" && (!(data3 % 1) && !isNaN(data3)) && isFinite(data3))) {
                    validate64.errors = [{ instancePath: instancePath + "/errorCount", schemaPath: "#/$defs/safe_count/type", keyword: "type", params: { type: "integer" }, message: "must be integer" }];
                    return false;
                  }
                  if (errors === _errs11) {
                    if (typeof data3 == "number" && isFinite(data3)) {
                      if (data3 > 9007199254740991 || isNaN(data3)) {
                        validate64.errors = [{ instancePath: instancePath + "/errorCount", schemaPath: "#/$defs/safe_count/maximum", keyword: "maximum", params: { comparison: "<=", limit: 9007199254740991 }, message: "must be <= 9007199254740991" }];
                        return false;
                      } else {
                        if (data3 < 0 || isNaN(data3)) {
                          validate64.errors = [{ instancePath: instancePath + "/errorCount", schemaPath: "#/$defs/safe_count/minimum", keyword: "minimum", params: { comparison: ">=", limit: 0 }, message: "must be >= 0" }];
                          return false;
                        }
                      }
                    }
                  }
                  var valid0 = _errs10 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.startTimeUnixMs !== void 0) {
                    let data4 = data.startTimeUnixMs;
                    const _errs13 = errors;
                    if (errors === _errs13) {
                      if (typeof data4 == "number" && isFinite(data4)) {
                        if (data4 < 0 || isNaN(data4)) {
                          validate64.errors = [{ instancePath: instancePath + "/startTimeUnixMs", schemaPath: "#/properties/startTimeUnixMs/minimum", keyword: "minimum", params: { comparison: ">=", limit: 0 }, message: "must be >= 0" }];
                          return false;
                        }
                      } else {
                        validate64.errors = [{ instancePath: instancePath + "/startTimeUnixMs", schemaPath: "#/properties/startTimeUnixMs/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                        return false;
                      }
                    }
                    var valid0 = _errs13 === errors;
                  } else {
                    var valid0 = true;
                  }
                  if (valid0) {
                    if (data.endTimeUnixMs !== void 0) {
                      let data5 = data.endTimeUnixMs;
                      const _errs15 = errors;
                      const _errs16 = errors;
                      let valid4 = false;
                      let passing0 = null;
                      const _errs17 = errors;
                      if (errors === _errs17) {
                        if (typeof data5 == "number" && isFinite(data5)) {
                          if (data5 < 0 || isNaN(data5)) {
                            const err0 = { instancePath: instancePath + "/endTimeUnixMs", schemaPath: "#/properties/endTimeUnixMs/oneOf/0/minimum", keyword: "minimum", params: { comparison: ">=", limit: 0 }, message: "must be >= 0" };
                            if (vErrors === null) {
                              vErrors = [err0];
                            } else {
                              vErrors.push(err0);
                            }
                            errors++;
                          }
                        } else {
                          const err1 = { instancePath: instancePath + "/endTimeUnixMs", schemaPath: "#/properties/endTimeUnixMs/oneOf/0/type", keyword: "type", params: { type: "number" }, message: "must be number" };
                          if (vErrors === null) {
                            vErrors = [err1];
                          } else {
                            vErrors.push(err1);
                          }
                          errors++;
                        }
                      }
                      var _valid0 = _errs17 === errors;
                      if (_valid0) {
                        valid4 = true;
                        passing0 = 0;
                      }
                      const _errs19 = errors;
                      if (data5 !== null) {
                        const err2 = { instancePath: instancePath + "/endTimeUnixMs", schemaPath: "#/properties/endTimeUnixMs/oneOf/1/type", keyword: "type", params: { type: "null" }, message: "must be null" };
                        if (vErrors === null) {
                          vErrors = [err2];
                        } else {
                          vErrors.push(err2);
                        }
                        errors++;
                      }
                      var _valid0 = _errs19 === errors;
                      if (_valid0 && valid4) {
                        valid4 = false;
                        passing0 = [passing0, 1];
                      } else {
                        if (_valid0) {
                          valid4 = true;
                          passing0 = 1;
                        }
                      }
                      if (!valid4) {
                        const err3 = { instancePath: instancePath + "/endTimeUnixMs", schemaPath: "#/properties/endTimeUnixMs/oneOf", keyword: "oneOf", params: { passingSchemas: passing0 }, message: "must match exactly one schema in oneOf" };
                        if (vErrors === null) {
                          vErrors = [err3];
                        } else {
                          vErrors.push(err3);
                        }
                        errors++;
                        validate64.errors = vErrors;
                        return false;
                      } else {
                        errors = _errs16;
                        if (vErrors !== null) {
                          if (_errs16) {
                            vErrors.length = _errs16;
                          } else {
                            vErrors = null;
                          }
                        }
                      }
                      var valid0 = _errs15 === errors;
                    } else {
                      var valid0 = true;
                    }
                    if (valid0) {
                      if (data.availabilityReasons !== void 0) {
                        const _errs21 = errors;
                        if (!validate65(data.availabilityReasons, { instancePath: instancePath + "/availabilityReasons", parentData: data, parentDataProperty: "availabilityReasons", rootData, dynamicAnchors })) {
                          vErrors = vErrors === null ? validate65.errors : vErrors.concat(validate65.errors);
                          errors = vErrors.length;
                        }
                        var valid0 = _errs21 === errors;
                      } else {
                        var valid0 = true;
                      }
                    }
                  }
                }
              }
            }
          }
        }
      }
    } else {
      validate64.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate64.errors = vErrors;
  return errors === 0;
}
validate64.evaluated = { "props": true, "dynamicProps": false, "dynamicItems": false };
var schema131 = { "title": "DashboardPaginationV1", "type": "object", "additionalProperties": false, "required": ["nextCursor"], "properties": { "nextCursor": { "$ref": "#/$defs/nullable_cursor" }, "total": { "$ref": "#/$defs/safe_count" } } };
var schema132 = { "oneOf": [{ "$ref": "#/$defs/cursor" }, { "type": "null" }] };
function validate87(data, { instancePath = "", parentData, parentDataProperty, rootData = data, dynamicAnchors = {} } = {}) {
  let vErrors = null;
  let errors = 0;
  const evaluated0 = validate87.evaluated;
  if (evaluated0.dynamicProps) {
    evaluated0.props = void 0;
  }
  if (evaluated0.dynamicItems) {
    evaluated0.items = void 0;
  }
  const _errs0 = errors;
  let valid0 = false;
  let passing0 = null;
  const _errs1 = errors;
  const _errs2 = errors;
  if (errors === _errs2) {
    if (typeof data === "string") {
      if (func1(data) > 2048) {
        const err0 = { instancePath, schemaPath: "#/$defs/cursor/maxLength", keyword: "maxLength", params: { limit: 2048 }, message: "must NOT have more than 2048 characters" };
        if (vErrors === null) {
          vErrors = [err0];
        } else {
          vErrors.push(err0);
        }
        errors++;
      } else {
        if (func1(data) < 1) {
          const err1 = { instancePath, schemaPath: "#/$defs/cursor/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" };
          if (vErrors === null) {
            vErrors = [err1];
          } else {
            vErrors.push(err1);
          }
          errors++;
        }
      }
    } else {
      const err2 = { instancePath, schemaPath: "#/$defs/cursor/type", keyword: "type", params: { type: "string" }, message: "must be string" };
      if (vErrors === null) {
        vErrors = [err2];
      } else {
        vErrors.push(err2);
      }
      errors++;
    }
  }
  var _valid0 = _errs1 === errors;
  if (_valid0) {
    valid0 = true;
    passing0 = 0;
  }
  const _errs4 = errors;
  if (data !== null) {
    const err3 = { instancePath, schemaPath: "#/oneOf/1/type", keyword: "type", params: { type: "null" }, message: "must be null" };
    if (vErrors === null) {
      vErrors = [err3];
    } else {
      vErrors.push(err3);
    }
    errors++;
  }
  var _valid0 = _errs4 === errors;
  if (_valid0 && valid0) {
    valid0 = false;
    passing0 = [passing0, 1];
  } else {
    if (_valid0) {
      valid0 = true;
      passing0 = 1;
    }
  }
  if (!valid0) {
    const err4 = { instancePath, schemaPath: "#/oneOf", keyword: "oneOf", params: { passingSchemas: passing0 }, message: "must match exactly one schema in oneOf" };
    if (vErrors === null) {
      vErrors = [err4];
    } else {
      vErrors.push(err4);
    }
    errors++;
    validate87.errors = vErrors;
    return false;
  } else {
    errors = _errs0;
    if (vErrors !== null) {
      if (_errs0) {
        vErrors.length = _errs0;
      } else {
        vErrors = null;
      }
    }
  }
  validate87.errors = vErrors;
  return errors === 0;
}
validate87.evaluated = { "dynamicProps": false, "dynamicItems": false };
function validate86(data, { instancePath = "", parentData, parentDataProperty, rootData = data, dynamicAnchors = {} } = {}) {
  let vErrors = null;
  let errors = 0;
  const evaluated0 = validate86.evaluated;
  if (evaluated0.dynamicProps) {
    evaluated0.props = void 0;
  }
  if (evaluated0.dynamicItems) {
    evaluated0.items = void 0;
  }
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.nextCursor === void 0 && (missing0 = "nextCursor")) {
        validate86.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "nextCursor" || key0 === "total")) {
            validate86.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.nextCursor !== void 0) {
            const _errs2 = errors;
            if (!validate87(data.nextCursor, { instancePath: instancePath + "/nextCursor", parentData: data, parentDataProperty: "nextCursor", rootData, dynamicAnchors })) {
              vErrors = vErrors === null ? validate87.errors : vErrors.concat(validate87.errors);
              errors = vErrors.length;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.total !== void 0) {
              let data1 = data.total;
              const _errs3 = errors;
              const _errs4 = errors;
              if (!(typeof data1 == "number" && (!(data1 % 1) && !isNaN(data1)) && isFinite(data1))) {
                validate86.errors = [{ instancePath: instancePath + "/total", schemaPath: "#/$defs/safe_count/type", keyword: "type", params: { type: "integer" }, message: "must be integer" }];
                return false;
              }
              if (errors === _errs4) {
                if (typeof data1 == "number" && isFinite(data1)) {
                  if (data1 > 9007199254740991 || isNaN(data1)) {
                    validate86.errors = [{ instancePath: instancePath + "/total", schemaPath: "#/$defs/safe_count/maximum", keyword: "maximum", params: { comparison: "<=", limit: 9007199254740991 }, message: "must be <= 9007199254740991" }];
                    return false;
                  } else {
                    if (data1 < 0 || isNaN(data1)) {
                      validate86.errors = [{ instancePath: instancePath + "/total", schemaPath: "#/$defs/safe_count/minimum", keyword: "minimum", params: { comparison: ">=", limit: 0 }, message: "must be >= 0" }];
                      return false;
                    }
                  }
                }
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
          }
        }
      }
    } else {
      validate86.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate86.errors = vErrors;
  return errors === 0;
}
validate86.evaluated = { "props": true, "dynamicProps": false, "dynamicItems": false };
function validate62(data, { instancePath = "", parentData, parentDataProperty, rootData = data, dynamicAnchors = {} } = {}) {
  let vErrors = null;
  let errors = 0;
  const evaluated0 = validate62.evaluated;
  if (evaluated0.dynamicProps) {
    evaluated0.props = void 0;
  }
  if (evaluated0.dynamicItems) {
    evaluated0.items = void 0;
  }
  const _errs1 = errors;
  if (!validate46(data, { instancePath, parentData, parentDataProperty, rootData, dynamicAnchors })) {
    vErrors = vErrors === null ? validate46.errors : vErrors.concat(validate46.errors);
    errors = vErrors.length;
  }
  var valid0 = _errs1 === errors;
  if (valid0) {
    const _errs2 = errors;
    if (errors === _errs2) {
      if (data && typeof data == "object" && !Array.isArray(data)) {
        let missing0;
        if (data.kind === void 0 && (missing0 = "kind") || data.rows === void 0 && (missing0 = "rows") || data.pagination === void 0 && (missing0 = "pagination")) {
          validate62.errors = [{ instancePath, schemaPath: "#/allOf/1/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
          return false;
        } else {
          if (data.kind !== void 0) {
            const _errs4 = errors;
            if ("traces" !== data.kind) {
              validate62.errors = [{ instancePath: instancePath + "/kind", schemaPath: "#/allOf/1/properties/kind/const", keyword: "const", params: { allowedValue: "traces" }, message: "must be equal to constant" }];
              return false;
            }
            var valid1 = _errs4 === errors;
          } else {
            var valid1 = true;
          }
          if (valid1) {
            if (data.rows !== void 0) {
              let data1 = data.rows;
              const _errs5 = errors;
              if (errors === _errs5) {
                if (Array.isArray(data1)) {
                  if (data1.length > 100) {
                    validate62.errors = [{ instancePath: instancePath + "/rows", schemaPath: "#/allOf/1/properties/rows/maxItems", keyword: "maxItems", params: { limit: 100 }, message: "must NOT have more than 100 items" }];
                    return false;
                  } else {
                    var valid2 = true;
                    const len0 = data1.length;
                    for (let i0 = 0; i0 < len0; i0++) {
                      const _errs7 = errors;
                      if (!validate64(data1[i0], { instancePath: instancePath + "/rows/" + i0, parentData: data1, parentDataProperty: i0, rootData, dynamicAnchors })) {
                        vErrors = vErrors === null ? validate64.errors : vErrors.concat(validate64.errors);
                        errors = vErrors.length;
                      }
                      var valid2 = _errs7 === errors;
                      if (!valid2) {
                        break;
                      }
                    }
                  }
                } else {
                  validate62.errors = [{ instancePath: instancePath + "/rows", schemaPath: "#/allOf/1/properties/rows/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                  return false;
                }
              }
              var valid1 = _errs5 === errors;
            } else {
              var valid1 = true;
            }
            if (valid1) {
              if (data.pagination !== void 0) {
                const _errs8 = errors;
                if (!validate86(data.pagination, { instancePath: instancePath + "/pagination", parentData: data, parentDataProperty: "pagination", rootData, dynamicAnchors })) {
                  vErrors = vErrors === null ? validate86.errors : vErrors.concat(validate86.errors);
                  errors = vErrors.length;
                }
                var valid1 = _errs8 === errors;
              } else {
                var valid1 = true;
              }
            }
          }
        }
      } else {
        validate62.errors = [{ instancePath, schemaPath: "#/allOf/1/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
        return false;
      }
    }
    var valid0 = _errs2 === errors;
  }
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      for (const key0 in data) {
        if (key0 !== "kind" && key0 !== "rows" && key0 !== "pagination" && key0 !== "schemaVersion" && key0 !== "snapshot" && key0 !== "scope" && key0 !== "work") {
          validate62.errors = [{ instancePath, schemaPath: "#/unevaluatedProperties", keyword: "unevaluatedProperties", params: { unevaluatedProperty: key0 }, message: "must NOT have unevaluated properties" }];
          return false;
          break;
        }
      }
    } else {
      validate62.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate62.errors = vErrors;
  return errors === 0;
}
validate62.evaluated = { "props": true, "dynamicProps": false, "dynamicItems": false };
var schema135 = { "title": "DashboardSpansResponseV1", "type": "object", "allOf": [{ "$ref": "#/$defs/response_base" }, { "type": "object", "required": ["kind", "rows", "pagination"], "properties": { "kind": { "const": "spans" }, "rows": { "type": "array", "maxItems": 200, "items": { "$ref": "#/$defs/span_row" } }, "pagination": { "$ref": "#/$defs/pagination" } } }], "unevaluatedProperties": false };
var schema136 = { "title": "DashboardSpanRowV1", "description": "Compact projected list row. It intentionally omits attributes, metrics, source content, response content and private raw detail.", "type": "object", "additionalProperties": false, "required": ["traceId", "spanId", "parentSpanId", "kind", "name", "status", "startTimeUnixMs", "endTimeUnixMs", "repo", "availabilityReasons"], "properties": { "traceId": { "$ref": "#/$defs/projected_id" }, "spanId": { "$ref": "#/$defs/projected_id" }, "parentSpanId": { "oneOf": [{ "$ref": "#/$defs/projected_id" }, { "type": "null" }] }, "kind": { "type": "string", "minLength": 1, "maxLength": 64 }, "name": { "type": "string", "minLength": 1, "maxLength": 256 }, "status": { "type": "string", "minLength": 1, "maxLength": 64 }, "startTimeUnixMs": { "type": "number", "minimum": 0 }, "endTimeUnixMs": { "oneOf": [{ "type": "number", "minimum": 0 }, { "type": "null" }] }, "repo": { "type": "string", "minLength": 1, "maxLength": 256 }, "agent": { "type": "string", "minLength": 1, "maxLength": 128 }, "model": { "type": "string", "minLength": 1, "maxLength": 256 }, "sessionId": { "$ref": "#/$defs/projected_id" }, "turnId": { "$ref": "#/$defs/projected_id" }, "toolName": { "type": "string", "minLength": 1, "maxLength": 128 }, "availabilityReasons": { "$ref": "#/$defs/availability_reasons" } } };
function validate93(data, { instancePath = "", parentData, parentDataProperty, rootData = data, dynamicAnchors = {} } = {}) {
  let vErrors = null;
  let errors = 0;
  const evaluated0 = validate93.evaluated;
  if (evaluated0.dynamicProps) {
    evaluated0.props = void 0;
  }
  if (evaluated0.dynamicItems) {
    evaluated0.items = void 0;
  }
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.traceId === void 0 && (missing0 = "traceId") || data.spanId === void 0 && (missing0 = "spanId") || data.parentSpanId === void 0 && (missing0 = "parentSpanId") || data.kind === void 0 && (missing0 = "kind") || data.name === void 0 && (missing0 = "name") || data.status === void 0 && (missing0 = "status") || data.startTimeUnixMs === void 0 && (missing0 = "startTimeUnixMs") || data.endTimeUnixMs === void 0 && (missing0 = "endTimeUnixMs") || data.repo === void 0 && (missing0 = "repo") || data.availabilityReasons === void 0 && (missing0 = "availabilityReasons")) {
        validate93.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!func27.call(schema136.properties, key0)) {
            validate93.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.traceId !== void 0) {
            let data0 = data.traceId;
            const _errs2 = errors;
            const _errs3 = errors;
            if (errors === _errs3) {
              if (typeof data0 === "string") {
                if (!pattern6.test(data0)) {
                  validate93.errors = [{ instancePath: instancePath + "/traceId", schemaPath: "#/$defs/projected_id/pattern", keyword: "pattern", params: { pattern: "^id:sha256:[0-9a-f]{64}$" }, message: 'must match pattern "^id:sha256:[0-9a-f]{64}$"' }];
                  return false;
                }
              } else {
                validate93.errors = [{ instancePath: instancePath + "/traceId", schemaPath: "#/$defs/projected_id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                return false;
              }
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.spanId !== void 0) {
              let data1 = data.spanId;
              const _errs5 = errors;
              const _errs6 = errors;
              if (errors === _errs6) {
                if (typeof data1 === "string") {
                  if (!pattern6.test(data1)) {
                    validate93.errors = [{ instancePath: instancePath + "/spanId", schemaPath: "#/$defs/projected_id/pattern", keyword: "pattern", params: { pattern: "^id:sha256:[0-9a-f]{64}$" }, message: 'must match pattern "^id:sha256:[0-9a-f]{64}$"' }];
                    return false;
                  }
                } else {
                  validate93.errors = [{ instancePath: instancePath + "/spanId", schemaPath: "#/$defs/projected_id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                  return false;
                }
              }
              var valid0 = _errs5 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.parentSpanId !== void 0) {
                let data2 = data.parentSpanId;
                const _errs8 = errors;
                const _errs9 = errors;
                let valid3 = false;
                let passing0 = null;
                const _errs10 = errors;
                const _errs11 = errors;
                if (errors === _errs11) {
                  if (typeof data2 === "string") {
                    if (!pattern6.test(data2)) {
                      const err0 = { instancePath: instancePath + "/parentSpanId", schemaPath: "#/$defs/projected_id/pattern", keyword: "pattern", params: { pattern: "^id:sha256:[0-9a-f]{64}$" }, message: 'must match pattern "^id:sha256:[0-9a-f]{64}$"' };
                      if (vErrors === null) {
                        vErrors = [err0];
                      } else {
                        vErrors.push(err0);
                      }
                      errors++;
                    }
                  } else {
                    const err1 = { instancePath: instancePath + "/parentSpanId", schemaPath: "#/$defs/projected_id/type", keyword: "type", params: { type: "string" }, message: "must be string" };
                    if (vErrors === null) {
                      vErrors = [err1];
                    } else {
                      vErrors.push(err1);
                    }
                    errors++;
                  }
                }
                var _valid0 = _errs10 === errors;
                if (_valid0) {
                  valid3 = true;
                  passing0 = 0;
                }
                const _errs13 = errors;
                if (data2 !== null) {
                  const err2 = { instancePath: instancePath + "/parentSpanId", schemaPath: "#/properties/parentSpanId/oneOf/1/type", keyword: "type", params: { type: "null" }, message: "must be null" };
                  if (vErrors === null) {
                    vErrors = [err2];
                  } else {
                    vErrors.push(err2);
                  }
                  errors++;
                }
                var _valid0 = _errs13 === errors;
                if (_valid0 && valid3) {
                  valid3 = false;
                  passing0 = [passing0, 1];
                } else {
                  if (_valid0) {
                    valid3 = true;
                    passing0 = 1;
                  }
                }
                if (!valid3) {
                  const err3 = { instancePath: instancePath + "/parentSpanId", schemaPath: "#/properties/parentSpanId/oneOf", keyword: "oneOf", params: { passingSchemas: passing0 }, message: "must match exactly one schema in oneOf" };
                  if (vErrors === null) {
                    vErrors = [err3];
                  } else {
                    vErrors.push(err3);
                  }
                  errors++;
                  validate93.errors = vErrors;
                  return false;
                } else {
                  errors = _errs9;
                  if (vErrors !== null) {
                    if (_errs9) {
                      vErrors.length = _errs9;
                    } else {
                      vErrors = null;
                    }
                  }
                }
                var valid0 = _errs8 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.kind !== void 0) {
                  let data3 = data.kind;
                  const _errs15 = errors;
                  if (errors === _errs15) {
                    if (typeof data3 === "string") {
                      if (func1(data3) > 64) {
                        validate93.errors = [{ instancePath: instancePath + "/kind", schemaPath: "#/properties/kind/maxLength", keyword: "maxLength", params: { limit: 64 }, message: "must NOT have more than 64 characters" }];
                        return false;
                      } else {
                        if (func1(data3) < 1) {
                          validate93.errors = [{ instancePath: instancePath + "/kind", schemaPath: "#/properties/kind/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                          return false;
                        }
                      }
                    } else {
                      validate93.errors = [{ instancePath: instancePath + "/kind", schemaPath: "#/properties/kind/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                      return false;
                    }
                  }
                  var valid0 = _errs15 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.name !== void 0) {
                    let data4 = data.name;
                    const _errs17 = errors;
                    if (errors === _errs17) {
                      if (typeof data4 === "string") {
                        if (func1(data4) > 256) {
                          validate93.errors = [{ instancePath: instancePath + "/name", schemaPath: "#/properties/name/maxLength", keyword: "maxLength", params: { limit: 256 }, message: "must NOT have more than 256 characters" }];
                          return false;
                        } else {
                          if (func1(data4) < 1) {
                            validate93.errors = [{ instancePath: instancePath + "/name", schemaPath: "#/properties/name/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                            return false;
                          }
                        }
                      } else {
                        validate93.errors = [{ instancePath: instancePath + "/name", schemaPath: "#/properties/name/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                        return false;
                      }
                    }
                    var valid0 = _errs17 === errors;
                  } else {
                    var valid0 = true;
                  }
                  if (valid0) {
                    if (data.status !== void 0) {
                      let data5 = data.status;
                      const _errs19 = errors;
                      if (errors === _errs19) {
                        if (typeof data5 === "string") {
                          if (func1(data5) > 64) {
                            validate93.errors = [{ instancePath: instancePath + "/status", schemaPath: "#/properties/status/maxLength", keyword: "maxLength", params: { limit: 64 }, message: "must NOT have more than 64 characters" }];
                            return false;
                          } else {
                            if (func1(data5) < 1) {
                              validate93.errors = [{ instancePath: instancePath + "/status", schemaPath: "#/properties/status/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                              return false;
                            }
                          }
                        } else {
                          validate93.errors = [{ instancePath: instancePath + "/status", schemaPath: "#/properties/status/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                          return false;
                        }
                      }
                      var valid0 = _errs19 === errors;
                    } else {
                      var valid0 = true;
                    }
                    if (valid0) {
                      if (data.startTimeUnixMs !== void 0) {
                        let data6 = data.startTimeUnixMs;
                        const _errs21 = errors;
                        if (errors === _errs21) {
                          if (typeof data6 == "number" && isFinite(data6)) {
                            if (data6 < 0 || isNaN(data6)) {
                              validate93.errors = [{ instancePath: instancePath + "/startTimeUnixMs", schemaPath: "#/properties/startTimeUnixMs/minimum", keyword: "minimum", params: { comparison: ">=", limit: 0 }, message: "must be >= 0" }];
                              return false;
                            }
                          } else {
                            validate93.errors = [{ instancePath: instancePath + "/startTimeUnixMs", schemaPath: "#/properties/startTimeUnixMs/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                            return false;
                          }
                        }
                        var valid0 = _errs21 === errors;
                      } else {
                        var valid0 = true;
                      }
                      if (valid0) {
                        if (data.endTimeUnixMs !== void 0) {
                          let data7 = data.endTimeUnixMs;
                          const _errs23 = errors;
                          const _errs24 = errors;
                          let valid5 = false;
                          let passing1 = null;
                          const _errs25 = errors;
                          if (errors === _errs25) {
                            if (typeof data7 == "number" && isFinite(data7)) {
                              if (data7 < 0 || isNaN(data7)) {
                                const err4 = { instancePath: instancePath + "/endTimeUnixMs", schemaPath: "#/properties/endTimeUnixMs/oneOf/0/minimum", keyword: "minimum", params: { comparison: ">=", limit: 0 }, message: "must be >= 0" };
                                if (vErrors === null) {
                                  vErrors = [err4];
                                } else {
                                  vErrors.push(err4);
                                }
                                errors++;
                              }
                            } else {
                              const err5 = { instancePath: instancePath + "/endTimeUnixMs", schemaPath: "#/properties/endTimeUnixMs/oneOf/0/type", keyword: "type", params: { type: "number" }, message: "must be number" };
                              if (vErrors === null) {
                                vErrors = [err5];
                              } else {
                                vErrors.push(err5);
                              }
                              errors++;
                            }
                          }
                          var _valid1 = _errs25 === errors;
                          if (_valid1) {
                            valid5 = true;
                            passing1 = 0;
                          }
                          const _errs27 = errors;
                          if (data7 !== null) {
                            const err6 = { instancePath: instancePath + "/endTimeUnixMs", schemaPath: "#/properties/endTimeUnixMs/oneOf/1/type", keyword: "type", params: { type: "null" }, message: "must be null" };
                            if (vErrors === null) {
                              vErrors = [err6];
                            } else {
                              vErrors.push(err6);
                            }
                            errors++;
                          }
                          var _valid1 = _errs27 === errors;
                          if (_valid1 && valid5) {
                            valid5 = false;
                            passing1 = [passing1, 1];
                          } else {
                            if (_valid1) {
                              valid5 = true;
                              passing1 = 1;
                            }
                          }
                          if (!valid5) {
                            const err7 = { instancePath: instancePath + "/endTimeUnixMs", schemaPath: "#/properties/endTimeUnixMs/oneOf", keyword: "oneOf", params: { passingSchemas: passing1 }, message: "must match exactly one schema in oneOf" };
                            if (vErrors === null) {
                              vErrors = [err7];
                            } else {
                              vErrors.push(err7);
                            }
                            errors++;
                            validate93.errors = vErrors;
                            return false;
                          } else {
                            errors = _errs24;
                            if (vErrors !== null) {
                              if (_errs24) {
                                vErrors.length = _errs24;
                              } else {
                                vErrors = null;
                              }
                            }
                          }
                          var valid0 = _errs23 === errors;
                        } else {
                          var valid0 = true;
                        }
                        if (valid0) {
                          if (data.repo !== void 0) {
                            let data8 = data.repo;
                            const _errs29 = errors;
                            if (errors === _errs29) {
                              if (typeof data8 === "string") {
                                if (func1(data8) > 256) {
                                  validate93.errors = [{ instancePath: instancePath + "/repo", schemaPath: "#/properties/repo/maxLength", keyword: "maxLength", params: { limit: 256 }, message: "must NOT have more than 256 characters" }];
                                  return false;
                                } else {
                                  if (func1(data8) < 1) {
                                    validate93.errors = [{ instancePath: instancePath + "/repo", schemaPath: "#/properties/repo/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                                    return false;
                                  }
                                }
                              } else {
                                validate93.errors = [{ instancePath: instancePath + "/repo", schemaPath: "#/properties/repo/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                return false;
                              }
                            }
                            var valid0 = _errs29 === errors;
                          } else {
                            var valid0 = true;
                          }
                          if (valid0) {
                            if (data.agent !== void 0) {
                              let data9 = data.agent;
                              const _errs31 = errors;
                              if (errors === _errs31) {
                                if (typeof data9 === "string") {
                                  if (func1(data9) > 128) {
                                    validate93.errors = [{ instancePath: instancePath + "/agent", schemaPath: "#/properties/agent/maxLength", keyword: "maxLength", params: { limit: 128 }, message: "must NOT have more than 128 characters" }];
                                    return false;
                                  } else {
                                    if (func1(data9) < 1) {
                                      validate93.errors = [{ instancePath: instancePath + "/agent", schemaPath: "#/properties/agent/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                                      return false;
                                    }
                                  }
                                } else {
                                  validate93.errors = [{ instancePath: instancePath + "/agent", schemaPath: "#/properties/agent/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                  return false;
                                }
                              }
                              var valid0 = _errs31 === errors;
                            } else {
                              var valid0 = true;
                            }
                            if (valid0) {
                              if (data.model !== void 0) {
                                let data10 = data.model;
                                const _errs33 = errors;
                                if (errors === _errs33) {
                                  if (typeof data10 === "string") {
                                    if (func1(data10) > 256) {
                                      validate93.errors = [{ instancePath: instancePath + "/model", schemaPath: "#/properties/model/maxLength", keyword: "maxLength", params: { limit: 256 }, message: "must NOT have more than 256 characters" }];
                                      return false;
                                    } else {
                                      if (func1(data10) < 1) {
                                        validate93.errors = [{ instancePath: instancePath + "/model", schemaPath: "#/properties/model/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                                        return false;
                                      }
                                    }
                                  } else {
                                    validate93.errors = [{ instancePath: instancePath + "/model", schemaPath: "#/properties/model/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                    return false;
                                  }
                                }
                                var valid0 = _errs33 === errors;
                              } else {
                                var valid0 = true;
                              }
                              if (valid0) {
                                if (data.sessionId !== void 0) {
                                  let data11 = data.sessionId;
                                  const _errs35 = errors;
                                  const _errs36 = errors;
                                  if (errors === _errs36) {
                                    if (typeof data11 === "string") {
                                      if (!pattern6.test(data11)) {
                                        validate93.errors = [{ instancePath: instancePath + "/sessionId", schemaPath: "#/$defs/projected_id/pattern", keyword: "pattern", params: { pattern: "^id:sha256:[0-9a-f]{64}$" }, message: 'must match pattern "^id:sha256:[0-9a-f]{64}$"' }];
                                        return false;
                                      }
                                    } else {
                                      validate93.errors = [{ instancePath: instancePath + "/sessionId", schemaPath: "#/$defs/projected_id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                      return false;
                                    }
                                  }
                                  var valid0 = _errs35 === errors;
                                } else {
                                  var valid0 = true;
                                }
                                if (valid0) {
                                  if (data.turnId !== void 0) {
                                    let data12 = data.turnId;
                                    const _errs38 = errors;
                                    const _errs39 = errors;
                                    if (errors === _errs39) {
                                      if (typeof data12 === "string") {
                                        if (!pattern6.test(data12)) {
                                          validate93.errors = [{ instancePath: instancePath + "/turnId", schemaPath: "#/$defs/projected_id/pattern", keyword: "pattern", params: { pattern: "^id:sha256:[0-9a-f]{64}$" }, message: 'must match pattern "^id:sha256:[0-9a-f]{64}$"' }];
                                          return false;
                                        }
                                      } else {
                                        validate93.errors = [{ instancePath: instancePath + "/turnId", schemaPath: "#/$defs/projected_id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                        return false;
                                      }
                                    }
                                    var valid0 = _errs38 === errors;
                                  } else {
                                    var valid0 = true;
                                  }
                                  if (valid0) {
                                    if (data.toolName !== void 0) {
                                      let data13 = data.toolName;
                                      const _errs41 = errors;
                                      if (errors === _errs41) {
                                        if (typeof data13 === "string") {
                                          if (func1(data13) > 128) {
                                            validate93.errors = [{ instancePath: instancePath + "/toolName", schemaPath: "#/properties/toolName/maxLength", keyword: "maxLength", params: { limit: 128 }, message: "must NOT have more than 128 characters" }];
                                            return false;
                                          } else {
                                            if (func1(data13) < 1) {
                                              validate93.errors = [{ instancePath: instancePath + "/toolName", schemaPath: "#/properties/toolName/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                                              return false;
                                            }
                                          }
                                        } else {
                                          validate93.errors = [{ instancePath: instancePath + "/toolName", schemaPath: "#/properties/toolName/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                          return false;
                                        }
                                      }
                                      var valid0 = _errs41 === errors;
                                    } else {
                                      var valid0 = true;
                                    }
                                    if (valid0) {
                                      if (data.availabilityReasons !== void 0) {
                                        const _errs43 = errors;
                                        if (!validate65(data.availabilityReasons, { instancePath: instancePath + "/availabilityReasons", parentData: data, parentDataProperty: "availabilityReasons", rootData, dynamicAnchors })) {
                                          vErrors = vErrors === null ? validate65.errors : vErrors.concat(validate65.errors);
                                          errors = vErrors.length;
                                        }
                                        var valid0 = _errs43 === errors;
                                      } else {
                                        var valid0 = true;
                                      }
                                    }
                                  }
                                }
                              }
                            }
                          }
                        }
                      }
                    }
                  }
                }
              }
            }
          }
        }
      }
    } else {
      validate93.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate93.errors = vErrors;
  return errors === 0;
}
validate93.evaluated = { "props": true, "dynamicProps": false, "dynamicItems": false };
function validate91(data, { instancePath = "", parentData, parentDataProperty, rootData = data, dynamicAnchors = {} } = {}) {
  let vErrors = null;
  let errors = 0;
  const evaluated0 = validate91.evaluated;
  if (evaluated0.dynamicProps) {
    evaluated0.props = void 0;
  }
  if (evaluated0.dynamicItems) {
    evaluated0.items = void 0;
  }
  const _errs1 = errors;
  if (!validate46(data, { instancePath, parentData, parentDataProperty, rootData, dynamicAnchors })) {
    vErrors = vErrors === null ? validate46.errors : vErrors.concat(validate46.errors);
    errors = vErrors.length;
  }
  var valid0 = _errs1 === errors;
  if (valid0) {
    const _errs2 = errors;
    if (errors === _errs2) {
      if (data && typeof data == "object" && !Array.isArray(data)) {
        let missing0;
        if (data.kind === void 0 && (missing0 = "kind") || data.rows === void 0 && (missing0 = "rows") || data.pagination === void 0 && (missing0 = "pagination")) {
          validate91.errors = [{ instancePath, schemaPath: "#/allOf/1/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
          return false;
        } else {
          if (data.kind !== void 0) {
            const _errs4 = errors;
            if ("spans" !== data.kind) {
              validate91.errors = [{ instancePath: instancePath + "/kind", schemaPath: "#/allOf/1/properties/kind/const", keyword: "const", params: { allowedValue: "spans" }, message: "must be equal to constant" }];
              return false;
            }
            var valid1 = _errs4 === errors;
          } else {
            var valid1 = true;
          }
          if (valid1) {
            if (data.rows !== void 0) {
              let data1 = data.rows;
              const _errs5 = errors;
              if (errors === _errs5) {
                if (Array.isArray(data1)) {
                  if (data1.length > 200) {
                    validate91.errors = [{ instancePath: instancePath + "/rows", schemaPath: "#/allOf/1/properties/rows/maxItems", keyword: "maxItems", params: { limit: 200 }, message: "must NOT have more than 200 items" }];
                    return false;
                  } else {
                    var valid2 = true;
                    const len0 = data1.length;
                    for (let i0 = 0; i0 < len0; i0++) {
                      const _errs7 = errors;
                      if (!validate93(data1[i0], { instancePath: instancePath + "/rows/" + i0, parentData: data1, parentDataProperty: i0, rootData, dynamicAnchors })) {
                        vErrors = vErrors === null ? validate93.errors : vErrors.concat(validate93.errors);
                        errors = vErrors.length;
                      }
                      var valid2 = _errs7 === errors;
                      if (!valid2) {
                        break;
                      }
                    }
                  }
                } else {
                  validate91.errors = [{ instancePath: instancePath + "/rows", schemaPath: "#/allOf/1/properties/rows/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                  return false;
                }
              }
              var valid1 = _errs5 === errors;
            } else {
              var valid1 = true;
            }
            if (valid1) {
              if (data.pagination !== void 0) {
                const _errs8 = errors;
                if (!validate86(data.pagination, { instancePath: instancePath + "/pagination", parentData: data, parentDataProperty: "pagination", rootData, dynamicAnchors })) {
                  vErrors = vErrors === null ? validate86.errors : vErrors.concat(validate86.errors);
                  errors = vErrors.length;
                }
                var valid1 = _errs8 === errors;
              } else {
                var valid1 = true;
              }
            }
          }
        }
      } else {
        validate91.errors = [{ instancePath, schemaPath: "#/allOf/1/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
        return false;
      }
    }
    var valid0 = _errs2 === errors;
  }
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      for (const key0 in data) {
        if (key0 !== "kind" && key0 !== "rows" && key0 !== "pagination" && key0 !== "schemaVersion" && key0 !== "snapshot" && key0 !== "scope" && key0 !== "work") {
          validate91.errors = [{ instancePath, schemaPath: "#/unevaluatedProperties", keyword: "unevaluatedProperties", params: { unevaluatedProperty: key0 }, message: "must NOT have unevaluated properties" }];
          return false;
          break;
        }
      }
    } else {
      validate91.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate91.errors = vErrors;
  return errors === 0;
}
validate91.evaluated = { "props": true, "dynamicProps": false, "dynamicItems": false };
var schema142 = { "title": "DashboardSummaryResponseV1", "type": "object", "allOf": [{ "$ref": "#/$defs/response_base" }, { "type": "object", "required": ["kind", "pagination"], "properties": { "kind": { "const": "summary" }, "pagination": { "$ref": "#/$defs/pagination" } } }], "oneOf": [{ "type": "object", "properties": { "work": { "type": "object", "properties": { "state": { "const": "pending" } } }, "pagination": { "type": "object", "properties": { "nextCursor": { "$ref": "#/$defs/cursor" } } } } }, { "type": "object", "properties": { "work": { "type": "object", "properties": { "state": { "const": "complete" } } }, "pagination": { "type": "object", "properties": { "nextCursor": { "type": "null" } } } } }], "unevaluatedProperties": false };
function validate98(data, { instancePath = "", parentData, parentDataProperty, rootData = data, dynamicAnchors = {} } = {}) {
  let vErrors = null;
  let errors = 0;
  const evaluated0 = validate98.evaluated;
  if (evaluated0.dynamicProps) {
    evaluated0.props = void 0;
  }
  if (evaluated0.dynamicItems) {
    evaluated0.items = void 0;
  }
  const _errs1 = errors;
  let valid0 = false;
  let passing0 = null;
  const _errs2 = errors;
  if (errors === _errs2) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      if (data.work !== void 0) {
        let data0 = data.work;
        const _errs4 = errors;
        if (errors === _errs4) {
          if (data0 && typeof data0 == "object" && !Array.isArray(data0)) {
            if (data0.state !== void 0) {
              if ("pending" !== data0.state) {
                const err0 = { instancePath: instancePath + "/work/state", schemaPath: "#/oneOf/0/properties/work/properties/state/const", keyword: "const", params: { allowedValue: "pending" }, message: "must be equal to constant" };
                if (vErrors === null) {
                  vErrors = [err0];
                } else {
                  vErrors.push(err0);
                }
                errors++;
              }
            }
          } else {
            const err1 = { instancePath: instancePath + "/work", schemaPath: "#/oneOf/0/properties/work/type", keyword: "type", params: { type: "object" }, message: "must be object" };
            if (vErrors === null) {
              vErrors = [err1];
            } else {
              vErrors.push(err1);
            }
            errors++;
          }
        }
        var valid1 = _errs4 === errors;
      } else {
        var valid1 = true;
      }
      if (valid1) {
        if (data.pagination !== void 0) {
          let data2 = data.pagination;
          const _errs7 = errors;
          if (errors === _errs7) {
            if (data2 && typeof data2 == "object" && !Array.isArray(data2)) {
              if (data2.nextCursor !== void 0) {
                let data3 = data2.nextCursor;
                const _errs10 = errors;
                if (errors === _errs10) {
                  if (typeof data3 === "string") {
                    if (func1(data3) > 2048) {
                      const err2 = { instancePath: instancePath + "/pagination/nextCursor", schemaPath: "#/$defs/cursor/maxLength", keyword: "maxLength", params: { limit: 2048 }, message: "must NOT have more than 2048 characters" };
                      if (vErrors === null) {
                        vErrors = [err2];
                      } else {
                        vErrors.push(err2);
                      }
                      errors++;
                    } else {
                      if (func1(data3) < 1) {
                        const err3 = { instancePath: instancePath + "/pagination/nextCursor", schemaPath: "#/$defs/cursor/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" };
                        if (vErrors === null) {
                          vErrors = [err3];
                        } else {
                          vErrors.push(err3);
                        }
                        errors++;
                      }
                    }
                  } else {
                    const err4 = { instancePath: instancePath + "/pagination/nextCursor", schemaPath: "#/$defs/cursor/type", keyword: "type", params: { type: "string" }, message: "must be string" };
                    if (vErrors === null) {
                      vErrors = [err4];
                    } else {
                      vErrors.push(err4);
                    }
                    errors++;
                  }
                }
              }
            } else {
              const err5 = { instancePath: instancePath + "/pagination", schemaPath: "#/oneOf/0/properties/pagination/type", keyword: "type", params: { type: "object" }, message: "must be object" };
              if (vErrors === null) {
                vErrors = [err5];
              } else {
                vErrors.push(err5);
              }
              errors++;
            }
          }
          var valid1 = _errs7 === errors;
        } else {
          var valid1 = true;
        }
      }
    } else {
      const err6 = { instancePath, schemaPath: "#/oneOf/0/type", keyword: "type", params: { type: "object" }, message: "must be object" };
      if (vErrors === null) {
        vErrors = [err6];
      } else {
        vErrors.push(err6);
      }
      errors++;
    }
  }
  var _valid0 = _errs2 === errors;
  if (_valid0) {
    valid0 = true;
    passing0 = 0;
    var props0 = {};
    props0.work = true;
    props0.pagination = true;
  }
  const _errs12 = errors;
  if (errors === _errs12) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      if (data.work !== void 0) {
        let data4 = data.work;
        const _errs14 = errors;
        if (errors === _errs14) {
          if (data4 && typeof data4 == "object" && !Array.isArray(data4)) {
            if (data4.state !== void 0) {
              if ("complete" !== data4.state) {
                const err7 = { instancePath: instancePath + "/work/state", schemaPath: "#/oneOf/1/properties/work/properties/state/const", keyword: "const", params: { allowedValue: "complete" }, message: "must be equal to constant" };
                if (vErrors === null) {
                  vErrors = [err7];
                } else {
                  vErrors.push(err7);
                }
                errors++;
              }
            }
          } else {
            const err8 = { instancePath: instancePath + "/work", schemaPath: "#/oneOf/1/properties/work/type", keyword: "type", params: { type: "object" }, message: "must be object" };
            if (vErrors === null) {
              vErrors = [err8];
            } else {
              vErrors.push(err8);
            }
            errors++;
          }
        }
        var valid5 = _errs14 === errors;
      } else {
        var valid5 = true;
      }
      if (valid5) {
        if (data.pagination !== void 0) {
          let data6 = data.pagination;
          const _errs17 = errors;
          if (errors === _errs17) {
            if (data6 && typeof data6 == "object" && !Array.isArray(data6)) {
              if (data6.nextCursor !== void 0) {
                if (data6.nextCursor !== null) {
                  const err9 = { instancePath: instancePath + "/pagination/nextCursor", schemaPath: "#/oneOf/1/properties/pagination/properties/nextCursor/type", keyword: "type", params: { type: "null" }, message: "must be null" };
                  if (vErrors === null) {
                    vErrors = [err9];
                  } else {
                    vErrors.push(err9);
                  }
                  errors++;
                }
              }
            } else {
              const err10 = { instancePath: instancePath + "/pagination", schemaPath: "#/oneOf/1/properties/pagination/type", keyword: "type", params: { type: "object" }, message: "must be object" };
              if (vErrors === null) {
                vErrors = [err10];
              } else {
                vErrors.push(err10);
              }
              errors++;
            }
          }
          var valid5 = _errs17 === errors;
        } else {
          var valid5 = true;
        }
      }
    } else {
      const err11 = { instancePath, schemaPath: "#/oneOf/1/type", keyword: "type", params: { type: "object" }, message: "must be object" };
      if (vErrors === null) {
        vErrors = [err11];
      } else {
        vErrors.push(err11);
      }
      errors++;
    }
  }
  var _valid0 = _errs12 === errors;
  if (_valid0 && valid0) {
    valid0 = false;
    passing0 = [passing0, 1];
  } else {
    if (_valid0) {
      valid0 = true;
      passing0 = 1;
      if (props0 !== true) {
        props0 = props0 || {};
        props0.work = true;
        props0.pagination = true;
      }
    }
  }
  if (!valid0) {
    const err12 = { instancePath, schemaPath: "#/oneOf", keyword: "oneOf", params: { passingSchemas: passing0 }, message: "must match exactly one schema in oneOf" };
    if (vErrors === null) {
      vErrors = [err12];
    } else {
      vErrors.push(err12);
    }
    errors++;
    validate98.errors = vErrors;
    return false;
  } else {
    errors = _errs1;
    if (vErrors !== null) {
      if (_errs1) {
        vErrors.length = _errs1;
      } else {
        vErrors = null;
      }
    }
    const _errs21 = errors;
    if (!validate46(data, { instancePath, parentData, parentDataProperty, rootData, dynamicAnchors })) {
      vErrors = vErrors === null ? validate46.errors : vErrors.concat(validate46.errors);
      errors = vErrors.length;
    }
    var valid8 = _errs21 === errors;
    if (valid8) {
      if (props0 !== true) {
        props0 = props0 || {};
        props0.schemaVersion = true;
        props0.snapshot = true;
        props0.scope = true;
        props0.work = true;
      }
      const _errs22 = errors;
      if (errors === _errs22) {
        if (data && typeof data == "object" && !Array.isArray(data)) {
          let missing0;
          if (data.kind === void 0 && (missing0 = "kind") || data.pagination === void 0 && (missing0 = "pagination")) {
            validate98.errors = [{ instancePath, schemaPath: "#/allOf/1/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
            return false;
          } else {
            if (data.kind !== void 0) {
              const _errs24 = errors;
              if ("summary" !== data.kind) {
                validate98.errors = [{ instancePath: instancePath + "/kind", schemaPath: "#/allOf/1/properties/kind/const", keyword: "const", params: { allowedValue: "summary" }, message: "must be equal to constant" }];
                return false;
              }
              var valid9 = _errs24 === errors;
            } else {
              var valid9 = true;
            }
            if (valid9) {
              if (data.pagination !== void 0) {
                const _errs25 = errors;
                if (!validate86(data.pagination, { instancePath: instancePath + "/pagination", parentData: data, parentDataProperty: "pagination", rootData, dynamicAnchors })) {
                  vErrors = vErrors === null ? validate86.errors : vErrors.concat(validate86.errors);
                  errors = vErrors.length;
                }
                var valid9 = _errs25 === errors;
              } else {
                var valid9 = true;
              }
            }
          }
        } else {
          validate98.errors = [{ instancePath, schemaPath: "#/allOf/1/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
          return false;
        }
      }
      var valid8 = _errs22 === errors;
      if (valid8) {
        if (props0 !== true) {
          props0 = props0 || {};
          props0.kind = true;
          props0.pagination = true;
        }
      }
    }
  }
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      if (props0 !== true) {
        for (const key0 in data) {
          if (!props0 || !props0[key0]) {
            validate98.errors = [{ instancePath, schemaPath: "#/unevaluatedProperties", keyword: "unevaluatedProperties", params: { unevaluatedProperty: key0 }, message: "must NOT have unevaluated properties" }];
            return false;
            break;
          }
        }
      }
    } else {
      validate98.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate98.errors = vErrors;
  return errors === 0;
}
validate98.evaluated = { "props": true, "dynamicProps": false, "dynamicItems": false };
var schema144 = { "title": "DashboardSpanResponseV1", "type": "object", "allOf": [{ "$ref": "#/$defs/response_base" }, { "type": "object", "required": ["kind", "detail"], "properties": { "kind": { "const": "span" }, "detail": { "allOf": [{ "$ref": "report-dto-v2.schema.json#/$defs/span" }, { "type": "object", "properties": { "traceId": { "$ref": "#/$defs/projected_id" }, "spanId": { "$ref": "#/$defs/projected_id" }, "parentSpanId": { "oneOf": [{ "$ref": "#/$defs/projected_id" }, { "type": "null" }] } } }] } } }], "unevaluatedProperties": false };
var schema104 = { "type": "object", "additionalProperties": false, "required": ["schemaVersion", "traceId", "spanId", "parentSpanId", "kind", "name", "status", "startTimeUnixMs", "endTimeUnixMs", "repo", "agent", "availability", "attributes", "metrics", "cost"], "properties": { "schemaVersion": { "type": "string" }, "traceId": { "type": "string" }, "spanId": { "type": "string" }, "parentSpanId": { "type": ["string", "null"] }, "kind": { "type": "string" }, "name": { "type": "string" }, "status": { "type": "string" }, "startTimeUnixMs": { "type": "number" }, "endTimeUnixMs": { "type": ["number", "null"] }, "repo": { "type": "string" }, "agent": { "$ref": "#/$defs/agent" }, "availability": { "$ref": "#/$defs/availability" }, "sessionId": { "type": "string" }, "turnId": { "type": "string" }, "toolName": { "type": "string" }, "attributes": { "$ref": "#/$defs/attributes" }, "metrics": { "$ref": "#/$defs/metrics" }, "estimatedCost": { "type": "number" }, "cost": { "$ref": "#/$defs/cost" } }, "allOf": [{ "if": { "properties": { "metrics": { "type": "object", "anyOf": [{ "type": "object", "properties": { "inputTokens": true }, "required": ["inputTokens"] }, { "type": "object", "properties": { "outputTokens": true }, "required": ["outputTokens"] }, { "type": "object", "properties": { "totalTokens": true }, "required": ["totalTokens"] }, { "type": "object", "properties": { "totalInputTokens": true }, "required": ["totalInputTokens"] }, { "type": "object", "properties": { "totalOutputTokens": true }, "required": ["totalOutputTokens"] }, { "type": "object", "properties": { "totalAccumulatedTokens": true }, "required": ["totalAccumulatedTokens"] }] } } }, "then": { "if": { "properties": { "metrics": { "type": "object", "anyOf": [{ "type": "object", "properties": { "totalTokens": true }, "required": ["totalTokens"] }, { "type": "object", "properties": { "totalAccumulatedTokens": true }, "required": ["totalAccumulatedTokens"] }, { "type": "object", "properties": { "inputTokens": true, "outputTokens": true }, "required": ["inputTokens", "outputTokens"] }, { "type": "object", "properties": { "totalInputTokens": true, "totalOutputTokens": true }, "required": ["totalInputTokens", "totalOutputTokens"] }] } } }, "then": { "properties": { "availability": { "type": "object", "properties": { "tokens": { "type": "object", "properties": { "state": { "const": "available" } } } } } } }, "else": { "properties": { "availability": { "type": "object", "properties": { "tokens": { "type": "object", "properties": { "state": { "const": "source_unavailable" }, "reason": { "const": "partial_token_metrics" } } } } } } } }, "else": { "properties": { "availability": { "type": "object", "properties": { "tokens": { "type": "object", "properties": { "state": { "not": { "const": "available" } } } } } } } } }] };
var schema105 = { "type": "object", "additionalProperties": false, "properties": { "name": { "type": "string" }, "model": { "type": "string" }, "version": { "type": "string" } } };
var schema128 = { "type": "object", "additionalProperties": false, "properties": { "inputTokens": { "type": "number" }, "outputTokens": { "type": "number" }, "cachedInputTokens": { "type": "number" }, "cacheCreationInputTokens": { "type": "number" }, "reasoningOutputTokens": { "type": "number" }, "totalTokens": { "type": "number" }, "latencyMs": { "type": "number" }, "durationMs": { "type": "number" }, "totalInputTokens": { "type": "number" }, "totalOutputTokens": { "type": "number" }, "totalCachedInputTokens": { "type": "number" }, "totalReasoningOutputTokens": { "type": "number" }, "totalAccumulatedTokens": { "type": "number" }, "contextWindowTokens": { "type": "number" } } };
var schema106 = { "type": "object", "additionalProperties": false, "required": ["repository", "turn", "model", "tokens", "latency", "sourceLocation", "requestContent", "responseContent"], "properties": { "repository": { "$ref": "#/$defs/field_availability" }, "turn": { "$ref": "#/$defs/field_availability" }, "model": { "$ref": "#/$defs/field_availability" }, "tokens": { "$ref": "#/$defs/field_availability" }, "latency": { "$ref": "#/$defs/field_availability" }, "sourceLocation": { "$ref": "#/$defs/field_availability" }, "requestContent": { "$ref": "#/$defs/field_availability" }, "responseContent": { "$ref": "#/$defs/field_availability" } } };
var schema107 = { "type": "object", "additionalProperties": false, "required": ["state", "reason"], "properties": { "state": { "enum": ["available", "source_unavailable", "withheld", "not_applicable", "private_lookup"] }, "reason": { "type": "string", "enum": ["reported_by_adapter", "derived_from_trace_context", "legacy_v1_report", "source_not_provided", "not_evaluated", "partial_token_metrics", "historical_codex_source_not_lookup_eligible", "codex_notify_turn_correlation_unavailable", "ambiguous_trace_repository", "span_kind_not_model_backed", "span_kind_has_no_latency", "span_kind_has_no_token_usage", "claude_private_lookup_not_supported", "cursor_private_lookup_not_supported", "codex_span_not_notify_derived", "agent_private_lookup_not_supported", "local_opt_in_lookup_required", "withheld_by_privacy_policy"] } }, "oneOf": [{ "properties": { "state": { "const": "available" }, "reason": { "enum": ["reported_by_adapter", "derived_from_trace_context", "legacy_v1_report"] } } }, { "properties": { "state": { "const": "source_unavailable" }, "reason": { "enum": ["source_not_provided", "not_evaluated", "partial_token_metrics", "historical_codex_source_not_lookup_eligible", "codex_notify_turn_correlation_unavailable", "ambiguous_trace_repository", "legacy_v1_report"] } } }, { "properties": { "state": { "const": "not_applicable" }, "reason": { "enum": ["span_kind_not_model_backed", "span_kind_has_no_latency", "span_kind_has_no_token_usage", "claude_private_lookup_not_supported", "cursor_private_lookup_not_supported", "codex_span_not_notify_derived", "agent_private_lookup_not_supported"] } } }, { "properties": { "state": { "const": "private_lookup" }, "reason": { "const": "local_opt_in_lookup_required" } } }, { "properties": { "state": { "const": "withheld" }, "reason": { "const": "withheld_by_privacy_policy" } } }] };
function validate77(data, { instancePath = "", parentData, parentDataProperty, rootData = data, dynamicAnchors = {} } = {}) {
  let vErrors = null;
  let errors = 0;
  const evaluated0 = validate77.evaluated;
  if (evaluated0.dynamicProps) {
    evaluated0.props = void 0;
  }
  if (evaluated0.dynamicItems) {
    evaluated0.items = void 0;
  }
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.repository === void 0 && (missing0 = "repository") || data.turn === void 0 && (missing0 = "turn") || data.model === void 0 && (missing0 = "model") || data.tokens === void 0 && (missing0 = "tokens") || data.latency === void 0 && (missing0 = "latency") || data.sourceLocation === void 0 && (missing0 = "sourceLocation") || data.requestContent === void 0 && (missing0 = "requestContent") || data.responseContent === void 0 && (missing0 = "responseContent")) {
        validate77.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "repository" || key0 === "turn" || key0 === "model" || key0 === "tokens" || key0 === "latency" || key0 === "sourceLocation" || key0 === "requestContent" || key0 === "responseContent")) {
            validate77.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.repository !== void 0) {
            let data0 = data.repository;
            const _errs2 = errors;
            const _errs3 = errors;
            const _errs5 = errors;
            let valid2 = false;
            let passing0 = null;
            const _errs6 = errors;
            if (data0 && typeof data0 == "object" && !Array.isArray(data0)) {
              if (data0.state !== void 0) {
                const _errs7 = errors;
                if ("available" !== data0.state) {
                  const err0 = { instancePath: instancePath + "/repository/state", schemaPath: "#/$defs/field_availability/oneOf/0/properties/state/const", keyword: "const", params: { allowedValue: "available" }, message: "must be equal to constant" };
                  if (vErrors === null) {
                    vErrors = [err0];
                  } else {
                    vErrors.push(err0);
                  }
                  errors++;
                }
                var valid3 = _errs7 === errors;
              } else {
                var valid3 = true;
              }
              if (valid3) {
                if (data0.reason !== void 0) {
                  let data2 = data0.reason;
                  const _errs8 = errors;
                  if (!(data2 === "reported_by_adapter" || data2 === "derived_from_trace_context" || data2 === "legacy_v1_report")) {
                    const err1 = { instancePath: instancePath + "/repository/reason", schemaPath: "#/$defs/field_availability/oneOf/0/properties/reason/enum", keyword: "enum", params: { allowedValues: schema107.oneOf[0].properties.reason.enum }, message: "must be equal to one of the allowed values" };
                    if (vErrors === null) {
                      vErrors = [err1];
                    } else {
                      vErrors.push(err1);
                    }
                    errors++;
                  }
                  var valid3 = _errs8 === errors;
                } else {
                  var valid3 = true;
                }
              }
            }
            var _valid0 = _errs6 === errors;
            if (_valid0) {
              valid2 = true;
              passing0 = 0;
              var props0 = {};
              props0.state = true;
              props0.reason = true;
            }
            const _errs9 = errors;
            if (data0 && typeof data0 == "object" && !Array.isArray(data0)) {
              if (data0.state !== void 0) {
                const _errs10 = errors;
                if ("source_unavailable" !== data0.state) {
                  const err2 = { instancePath: instancePath + "/repository/state", schemaPath: "#/$defs/field_availability/oneOf/1/properties/state/const", keyword: "const", params: { allowedValue: "source_unavailable" }, message: "must be equal to constant" };
                  if (vErrors === null) {
                    vErrors = [err2];
                  } else {
                    vErrors.push(err2);
                  }
                  errors++;
                }
                var valid4 = _errs10 === errors;
              } else {
                var valid4 = true;
              }
              if (valid4) {
                if (data0.reason !== void 0) {
                  let data4 = data0.reason;
                  const _errs11 = errors;
                  if (!(data4 === "source_not_provided" || data4 === "not_evaluated" || data4 === "partial_token_metrics" || data4 === "historical_codex_source_not_lookup_eligible" || data4 === "codex_notify_turn_correlation_unavailable" || data4 === "ambiguous_trace_repository" || data4 === "legacy_v1_report")) {
                    const err3 = { instancePath: instancePath + "/repository/reason", schemaPath: "#/$defs/field_availability/oneOf/1/properties/reason/enum", keyword: "enum", params: { allowedValues: schema107.oneOf[1].properties.reason.enum }, message: "must be equal to one of the allowed values" };
                    if (vErrors === null) {
                      vErrors = [err3];
                    } else {
                      vErrors.push(err3);
                    }
                    errors++;
                  }
                  var valid4 = _errs11 === errors;
                } else {
                  var valid4 = true;
                }
              }
            }
            var _valid0 = _errs9 === errors;
            if (_valid0 && valid2) {
              valid2 = false;
              passing0 = [passing0, 1];
            } else {
              if (_valid0) {
                valid2 = true;
                passing0 = 1;
                if (props0 !== true) {
                  props0 = props0 || {};
                  props0.state = true;
                  props0.reason = true;
                }
              }
              const _errs12 = errors;
              if (data0 && typeof data0 == "object" && !Array.isArray(data0)) {
                if (data0.state !== void 0) {
                  const _errs13 = errors;
                  if ("not_applicable" !== data0.state) {
                    const err4 = { instancePath: instancePath + "/repository/state", schemaPath: "#/$defs/field_availability/oneOf/2/properties/state/const", keyword: "const", params: { allowedValue: "not_applicable" }, message: "must be equal to constant" };
                    if (vErrors === null) {
                      vErrors = [err4];
                    } else {
                      vErrors.push(err4);
                    }
                    errors++;
                  }
                  var valid5 = _errs13 === errors;
                } else {
                  var valid5 = true;
                }
                if (valid5) {
                  if (data0.reason !== void 0) {
                    let data6 = data0.reason;
                    const _errs14 = errors;
                    if (!(data6 === "span_kind_not_model_backed" || data6 === "span_kind_has_no_latency" || data6 === "span_kind_has_no_token_usage" || data6 === "claude_private_lookup_not_supported" || data6 === "cursor_private_lookup_not_supported" || data6 === "codex_span_not_notify_derived" || data6 === "agent_private_lookup_not_supported")) {
                      const err5 = { instancePath: instancePath + "/repository/reason", schemaPath: "#/$defs/field_availability/oneOf/2/properties/reason/enum", keyword: "enum", params: { allowedValues: schema107.oneOf[2].properties.reason.enum }, message: "must be equal to one of the allowed values" };
                      if (vErrors === null) {
                        vErrors = [err5];
                      } else {
                        vErrors.push(err5);
                      }
                      errors++;
                    }
                    var valid5 = _errs14 === errors;
                  } else {
                    var valid5 = true;
                  }
                }
              }
              var _valid0 = _errs12 === errors;
              if (_valid0 && valid2) {
                valid2 = false;
                passing0 = [passing0, 2];
              } else {
                if (_valid0) {
                  valid2 = true;
                  passing0 = 2;
                  if (props0 !== true) {
                    props0 = props0 || {};
                    props0.state = true;
                    props0.reason = true;
                  }
                }
                const _errs15 = errors;
                if (data0 && typeof data0 == "object" && !Array.isArray(data0)) {
                  if (data0.state !== void 0) {
                    const _errs16 = errors;
                    if ("private_lookup" !== data0.state) {
                      const err6 = { instancePath: instancePath + "/repository/state", schemaPath: "#/$defs/field_availability/oneOf/3/properties/state/const", keyword: "const", params: { allowedValue: "private_lookup" }, message: "must be equal to constant" };
                      if (vErrors === null) {
                        vErrors = [err6];
                      } else {
                        vErrors.push(err6);
                      }
                      errors++;
                    }
                    var valid6 = _errs16 === errors;
                  } else {
                    var valid6 = true;
                  }
                  if (valid6) {
                    if (data0.reason !== void 0) {
                      const _errs17 = errors;
                      if ("local_opt_in_lookup_required" !== data0.reason) {
                        const err7 = { instancePath: instancePath + "/repository/reason", schemaPath: "#/$defs/field_availability/oneOf/3/properties/reason/const", keyword: "const", params: { allowedValue: "local_opt_in_lookup_required" }, message: "must be equal to constant" };
                        if (vErrors === null) {
                          vErrors = [err7];
                        } else {
                          vErrors.push(err7);
                        }
                        errors++;
                      }
                      var valid6 = _errs17 === errors;
                    } else {
                      var valid6 = true;
                    }
                  }
                }
                var _valid0 = _errs15 === errors;
                if (_valid0 && valid2) {
                  valid2 = false;
                  passing0 = [passing0, 3];
                } else {
                  if (_valid0) {
                    valid2 = true;
                    passing0 = 3;
                    if (props0 !== true) {
                      props0 = props0 || {};
                      props0.state = true;
                      props0.reason = true;
                    }
                  }
                  const _errs18 = errors;
                  if (data0 && typeof data0 == "object" && !Array.isArray(data0)) {
                    if (data0.state !== void 0) {
                      const _errs19 = errors;
                      if ("withheld" !== data0.state) {
                        const err8 = { instancePath: instancePath + "/repository/state", schemaPath: "#/$defs/field_availability/oneOf/4/properties/state/const", keyword: "const", params: { allowedValue: "withheld" }, message: "must be equal to constant" };
                        if (vErrors === null) {
                          vErrors = [err8];
                        } else {
                          vErrors.push(err8);
                        }
                        errors++;
                      }
                      var valid7 = _errs19 === errors;
                    } else {
                      var valid7 = true;
                    }
                    if (valid7) {
                      if (data0.reason !== void 0) {
                        const _errs20 = errors;
                        if ("withheld_by_privacy_policy" !== data0.reason) {
                          const err9 = { instancePath: instancePath + "/repository/reason", schemaPath: "#/$defs/field_availability/oneOf/4/properties/reason/const", keyword: "const", params: { allowedValue: "withheld_by_privacy_policy" }, message: "must be equal to constant" };
                          if (vErrors === null) {
                            vErrors = [err9];
                          } else {
                            vErrors.push(err9);
                          }
                          errors++;
                        }
                        var valid7 = _errs20 === errors;
                      } else {
                        var valid7 = true;
                      }
                    }
                  }
                  var _valid0 = _errs18 === errors;
                  if (_valid0 && valid2) {
                    valid2 = false;
                    passing0 = [passing0, 4];
                  } else {
                    if (_valid0) {
                      valid2 = true;
                      passing0 = 4;
                      if (props0 !== true) {
                        props0 = props0 || {};
                        props0.state = true;
                        props0.reason = true;
                      }
                    }
                  }
                }
              }
            }
            if (!valid2) {
              const err10 = { instancePath: instancePath + "/repository", schemaPath: "#/$defs/field_availability/oneOf", keyword: "oneOf", params: { passingSchemas: passing0 }, message: "must match exactly one schema in oneOf" };
              if (vErrors === null) {
                vErrors = [err10];
              } else {
                vErrors.push(err10);
              }
              errors++;
              validate77.errors = vErrors;
              return false;
            } else {
              errors = _errs5;
              if (vErrors !== null) {
                if (_errs5) {
                  vErrors.length = _errs5;
                } else {
                  vErrors = null;
                }
              }
            }
            if (errors === _errs3) {
              if (data0 && typeof data0 == "object" && !Array.isArray(data0)) {
                let missing1;
                if (data0.state === void 0 && (missing1 = "state") || data0.reason === void 0 && (missing1 = "reason")) {
                  validate77.errors = [{ instancePath: instancePath + "/repository", schemaPath: "#/$defs/field_availability/required", keyword: "required", params: { missingProperty: missing1 }, message: "must have required property '" + missing1 + "'" }];
                  return false;
                } else {
                  const _errs21 = errors;
                  for (const key1 in data0) {
                    if (!(key1 === "state" || key1 === "reason")) {
                      validate77.errors = [{ instancePath: instancePath + "/repository", schemaPath: "#/$defs/field_availability/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key1 }, message: "must NOT have additional properties" }];
                      return false;
                      break;
                    }
                  }
                  if (_errs21 === errors) {
                    if (data0.state !== void 0) {
                      let data11 = data0.state;
                      const _errs22 = errors;
                      if (!(data11 === "available" || data11 === "source_unavailable" || data11 === "withheld" || data11 === "not_applicable" || data11 === "private_lookup")) {
                        validate77.errors = [{ instancePath: instancePath + "/repository/state", schemaPath: "#/$defs/field_availability/properties/state/enum", keyword: "enum", params: { allowedValues: schema107.properties.state.enum }, message: "must be equal to one of the allowed values" }];
                        return false;
                      }
                      var valid8 = _errs22 === errors;
                    } else {
                      var valid8 = true;
                    }
                    if (valid8) {
                      if (data0.reason !== void 0) {
                        let data12 = data0.reason;
                        const _errs23 = errors;
                        if (typeof data12 !== "string") {
                          validate77.errors = [{ instancePath: instancePath + "/repository/reason", schemaPath: "#/$defs/field_availability/properties/reason/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                          return false;
                        }
                        if (!(data12 === "reported_by_adapter" || data12 === "derived_from_trace_context" || data12 === "legacy_v1_report" || data12 === "source_not_provided" || data12 === "not_evaluated" || data12 === "partial_token_metrics" || data12 === "historical_codex_source_not_lookup_eligible" || data12 === "codex_notify_turn_correlation_unavailable" || data12 === "ambiguous_trace_repository" || data12 === "span_kind_not_model_backed" || data12 === "span_kind_has_no_latency" || data12 === "span_kind_has_no_token_usage" || data12 === "claude_private_lookup_not_supported" || data12 === "cursor_private_lookup_not_supported" || data12 === "codex_span_not_notify_derived" || data12 === "agent_private_lookup_not_supported" || data12 === "local_opt_in_lookup_required" || data12 === "withheld_by_privacy_policy")) {
                          validate77.errors = [{ instancePath: instancePath + "/repository/reason", schemaPath: "#/$defs/field_availability/properties/reason/enum", keyword: "enum", params: { allowedValues: schema107.properties.reason.enum }, message: "must be equal to one of the allowed values" }];
                          return false;
                        }
                        var valid8 = _errs23 === errors;
                      } else {
                        var valid8 = true;
                      }
                    }
                  }
                }
              } else {
                validate77.errors = [{ instancePath: instancePath + "/repository", schemaPath: "#/$defs/field_availability/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
                return false;
              }
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.turn !== void 0) {
              let data13 = data.turn;
              const _errs25 = errors;
              const _errs26 = errors;
              const _errs28 = errors;
              let valid10 = false;
              let passing1 = null;
              const _errs29 = errors;
              if (data13 && typeof data13 == "object" && !Array.isArray(data13)) {
                if (data13.state !== void 0) {
                  const _errs30 = errors;
                  if ("available" !== data13.state) {
                    const err11 = { instancePath: instancePath + "/turn/state", schemaPath: "#/$defs/field_availability/oneOf/0/properties/state/const", keyword: "const", params: { allowedValue: "available" }, message: "must be equal to constant" };
                    if (vErrors === null) {
                      vErrors = [err11];
                    } else {
                      vErrors.push(err11);
                    }
                    errors++;
                  }
                  var valid11 = _errs30 === errors;
                } else {
                  var valid11 = true;
                }
                if (valid11) {
                  if (data13.reason !== void 0) {
                    let data15 = data13.reason;
                    const _errs31 = errors;
                    if (!(data15 === "reported_by_adapter" || data15 === "derived_from_trace_context" || data15 === "legacy_v1_report")) {
                      const err12 = { instancePath: instancePath + "/turn/reason", schemaPath: "#/$defs/field_availability/oneOf/0/properties/reason/enum", keyword: "enum", params: { allowedValues: schema107.oneOf[0].properties.reason.enum }, message: "must be equal to one of the allowed values" };
                      if (vErrors === null) {
                        vErrors = [err12];
                      } else {
                        vErrors.push(err12);
                      }
                      errors++;
                    }
                    var valid11 = _errs31 === errors;
                  } else {
                    var valid11 = true;
                  }
                }
              }
              var _valid1 = _errs29 === errors;
              if (_valid1) {
                valid10 = true;
                passing1 = 0;
                var props1 = {};
                props1.state = true;
                props1.reason = true;
              }
              const _errs32 = errors;
              if (data13 && typeof data13 == "object" && !Array.isArray(data13)) {
                if (data13.state !== void 0) {
                  const _errs33 = errors;
                  if ("source_unavailable" !== data13.state) {
                    const err13 = { instancePath: instancePath + "/turn/state", schemaPath: "#/$defs/field_availability/oneOf/1/properties/state/const", keyword: "const", params: { allowedValue: "source_unavailable" }, message: "must be equal to constant" };
                    if (vErrors === null) {
                      vErrors = [err13];
                    } else {
                      vErrors.push(err13);
                    }
                    errors++;
                  }
                  var valid12 = _errs33 === errors;
                } else {
                  var valid12 = true;
                }
                if (valid12) {
                  if (data13.reason !== void 0) {
                    let data17 = data13.reason;
                    const _errs34 = errors;
                    if (!(data17 === "source_not_provided" || data17 === "not_evaluated" || data17 === "partial_token_metrics" || data17 === "historical_codex_source_not_lookup_eligible" || data17 === "codex_notify_turn_correlation_unavailable" || data17 === "ambiguous_trace_repository" || data17 === "legacy_v1_report")) {
                      const err14 = { instancePath: instancePath + "/turn/reason", schemaPath: "#/$defs/field_availability/oneOf/1/properties/reason/enum", keyword: "enum", params: { allowedValues: schema107.oneOf[1].properties.reason.enum }, message: "must be equal to one of the allowed values" };
                      if (vErrors === null) {
                        vErrors = [err14];
                      } else {
                        vErrors.push(err14);
                      }
                      errors++;
                    }
                    var valid12 = _errs34 === errors;
                  } else {
                    var valid12 = true;
                  }
                }
              }
              var _valid1 = _errs32 === errors;
              if (_valid1 && valid10) {
                valid10 = false;
                passing1 = [passing1, 1];
              } else {
                if (_valid1) {
                  valid10 = true;
                  passing1 = 1;
                  if (props1 !== true) {
                    props1 = props1 || {};
                    props1.state = true;
                    props1.reason = true;
                  }
                }
                const _errs35 = errors;
                if (data13 && typeof data13 == "object" && !Array.isArray(data13)) {
                  if (data13.state !== void 0) {
                    const _errs36 = errors;
                    if ("not_applicable" !== data13.state) {
                      const err15 = { instancePath: instancePath + "/turn/state", schemaPath: "#/$defs/field_availability/oneOf/2/properties/state/const", keyword: "const", params: { allowedValue: "not_applicable" }, message: "must be equal to constant" };
                      if (vErrors === null) {
                        vErrors = [err15];
                      } else {
                        vErrors.push(err15);
                      }
                      errors++;
                    }
                    var valid13 = _errs36 === errors;
                  } else {
                    var valid13 = true;
                  }
                  if (valid13) {
                    if (data13.reason !== void 0) {
                      let data19 = data13.reason;
                      const _errs37 = errors;
                      if (!(data19 === "span_kind_not_model_backed" || data19 === "span_kind_has_no_latency" || data19 === "span_kind_has_no_token_usage" || data19 === "claude_private_lookup_not_supported" || data19 === "cursor_private_lookup_not_supported" || data19 === "codex_span_not_notify_derived" || data19 === "agent_private_lookup_not_supported")) {
                        const err16 = { instancePath: instancePath + "/turn/reason", schemaPath: "#/$defs/field_availability/oneOf/2/properties/reason/enum", keyword: "enum", params: { allowedValues: schema107.oneOf[2].properties.reason.enum }, message: "must be equal to one of the allowed values" };
                        if (vErrors === null) {
                          vErrors = [err16];
                        } else {
                          vErrors.push(err16);
                        }
                        errors++;
                      }
                      var valid13 = _errs37 === errors;
                    } else {
                      var valid13 = true;
                    }
                  }
                }
                var _valid1 = _errs35 === errors;
                if (_valid1 && valid10) {
                  valid10 = false;
                  passing1 = [passing1, 2];
                } else {
                  if (_valid1) {
                    valid10 = true;
                    passing1 = 2;
                    if (props1 !== true) {
                      props1 = props1 || {};
                      props1.state = true;
                      props1.reason = true;
                    }
                  }
                  const _errs38 = errors;
                  if (data13 && typeof data13 == "object" && !Array.isArray(data13)) {
                    if (data13.state !== void 0) {
                      const _errs39 = errors;
                      if ("private_lookup" !== data13.state) {
                        const err17 = { instancePath: instancePath + "/turn/state", schemaPath: "#/$defs/field_availability/oneOf/3/properties/state/const", keyword: "const", params: { allowedValue: "private_lookup" }, message: "must be equal to constant" };
                        if (vErrors === null) {
                          vErrors = [err17];
                        } else {
                          vErrors.push(err17);
                        }
                        errors++;
                      }
                      var valid14 = _errs39 === errors;
                    } else {
                      var valid14 = true;
                    }
                    if (valid14) {
                      if (data13.reason !== void 0) {
                        const _errs40 = errors;
                        if ("local_opt_in_lookup_required" !== data13.reason) {
                          const err18 = { instancePath: instancePath + "/turn/reason", schemaPath: "#/$defs/field_availability/oneOf/3/properties/reason/const", keyword: "const", params: { allowedValue: "local_opt_in_lookup_required" }, message: "must be equal to constant" };
                          if (vErrors === null) {
                            vErrors = [err18];
                          } else {
                            vErrors.push(err18);
                          }
                          errors++;
                        }
                        var valid14 = _errs40 === errors;
                      } else {
                        var valid14 = true;
                      }
                    }
                  }
                  var _valid1 = _errs38 === errors;
                  if (_valid1 && valid10) {
                    valid10 = false;
                    passing1 = [passing1, 3];
                  } else {
                    if (_valid1) {
                      valid10 = true;
                      passing1 = 3;
                      if (props1 !== true) {
                        props1 = props1 || {};
                        props1.state = true;
                        props1.reason = true;
                      }
                    }
                    const _errs41 = errors;
                    if (data13 && typeof data13 == "object" && !Array.isArray(data13)) {
                      if (data13.state !== void 0) {
                        const _errs42 = errors;
                        if ("withheld" !== data13.state) {
                          const err19 = { instancePath: instancePath + "/turn/state", schemaPath: "#/$defs/field_availability/oneOf/4/properties/state/const", keyword: "const", params: { allowedValue: "withheld" }, message: "must be equal to constant" };
                          if (vErrors === null) {
                            vErrors = [err19];
                          } else {
                            vErrors.push(err19);
                          }
                          errors++;
                        }
                        var valid15 = _errs42 === errors;
                      } else {
                        var valid15 = true;
                      }
                      if (valid15) {
                        if (data13.reason !== void 0) {
                          const _errs43 = errors;
                          if ("withheld_by_privacy_policy" !== data13.reason) {
                            const err20 = { instancePath: instancePath + "/turn/reason", schemaPath: "#/$defs/field_availability/oneOf/4/properties/reason/const", keyword: "const", params: { allowedValue: "withheld_by_privacy_policy" }, message: "must be equal to constant" };
                            if (vErrors === null) {
                              vErrors = [err20];
                            } else {
                              vErrors.push(err20);
                            }
                            errors++;
                          }
                          var valid15 = _errs43 === errors;
                        } else {
                          var valid15 = true;
                        }
                      }
                    }
                    var _valid1 = _errs41 === errors;
                    if (_valid1 && valid10) {
                      valid10 = false;
                      passing1 = [passing1, 4];
                    } else {
                      if (_valid1) {
                        valid10 = true;
                        passing1 = 4;
                        if (props1 !== true) {
                          props1 = props1 || {};
                          props1.state = true;
                          props1.reason = true;
                        }
                      }
                    }
                  }
                }
              }
              if (!valid10) {
                const err21 = { instancePath: instancePath + "/turn", schemaPath: "#/$defs/field_availability/oneOf", keyword: "oneOf", params: { passingSchemas: passing1 }, message: "must match exactly one schema in oneOf" };
                if (vErrors === null) {
                  vErrors = [err21];
                } else {
                  vErrors.push(err21);
                }
                errors++;
                validate77.errors = vErrors;
                return false;
              } else {
                errors = _errs28;
                if (vErrors !== null) {
                  if (_errs28) {
                    vErrors.length = _errs28;
                  } else {
                    vErrors = null;
                  }
                }
              }
              if (errors === _errs26) {
                if (data13 && typeof data13 == "object" && !Array.isArray(data13)) {
                  let missing2;
                  if (data13.state === void 0 && (missing2 = "state") || data13.reason === void 0 && (missing2 = "reason")) {
                    validate77.errors = [{ instancePath: instancePath + "/turn", schemaPath: "#/$defs/field_availability/required", keyword: "required", params: { missingProperty: missing2 }, message: "must have required property '" + missing2 + "'" }];
                    return false;
                  } else {
                    const _errs44 = errors;
                    for (const key2 in data13) {
                      if (!(key2 === "state" || key2 === "reason")) {
                        validate77.errors = [{ instancePath: instancePath + "/turn", schemaPath: "#/$defs/field_availability/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key2 }, message: "must NOT have additional properties" }];
                        return false;
                        break;
                      }
                    }
                    if (_errs44 === errors) {
                      if (data13.state !== void 0) {
                        let data24 = data13.state;
                        const _errs45 = errors;
                        if (!(data24 === "available" || data24 === "source_unavailable" || data24 === "withheld" || data24 === "not_applicable" || data24 === "private_lookup")) {
                          validate77.errors = [{ instancePath: instancePath + "/turn/state", schemaPath: "#/$defs/field_availability/properties/state/enum", keyword: "enum", params: { allowedValues: schema107.properties.state.enum }, message: "must be equal to one of the allowed values" }];
                          return false;
                        }
                        var valid16 = _errs45 === errors;
                      } else {
                        var valid16 = true;
                      }
                      if (valid16) {
                        if (data13.reason !== void 0) {
                          let data25 = data13.reason;
                          const _errs46 = errors;
                          if (typeof data25 !== "string") {
                            validate77.errors = [{ instancePath: instancePath + "/turn/reason", schemaPath: "#/$defs/field_availability/properties/reason/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                            return false;
                          }
                          if (!(data25 === "reported_by_adapter" || data25 === "derived_from_trace_context" || data25 === "legacy_v1_report" || data25 === "source_not_provided" || data25 === "not_evaluated" || data25 === "partial_token_metrics" || data25 === "historical_codex_source_not_lookup_eligible" || data25 === "codex_notify_turn_correlation_unavailable" || data25 === "ambiguous_trace_repository" || data25 === "span_kind_not_model_backed" || data25 === "span_kind_has_no_latency" || data25 === "span_kind_has_no_token_usage" || data25 === "claude_private_lookup_not_supported" || data25 === "cursor_private_lookup_not_supported" || data25 === "codex_span_not_notify_derived" || data25 === "agent_private_lookup_not_supported" || data25 === "local_opt_in_lookup_required" || data25 === "withheld_by_privacy_policy")) {
                            validate77.errors = [{ instancePath: instancePath + "/turn/reason", schemaPath: "#/$defs/field_availability/properties/reason/enum", keyword: "enum", params: { allowedValues: schema107.properties.reason.enum }, message: "must be equal to one of the allowed values" }];
                            return false;
                          }
                          var valid16 = _errs46 === errors;
                        } else {
                          var valid16 = true;
                        }
                      }
                    }
                  }
                } else {
                  validate77.errors = [{ instancePath: instancePath + "/turn", schemaPath: "#/$defs/field_availability/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
                  return false;
                }
              }
              var valid0 = _errs25 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.model !== void 0) {
                let data26 = data.model;
                const _errs48 = errors;
                const _errs49 = errors;
                const _errs51 = errors;
                let valid18 = false;
                let passing2 = null;
                const _errs52 = errors;
                if (data26 && typeof data26 == "object" && !Array.isArray(data26)) {
                  if (data26.state !== void 0) {
                    const _errs53 = errors;
                    if ("available" !== data26.state) {
                      const err22 = { instancePath: instancePath + "/model/state", schemaPath: "#/$defs/field_availability/oneOf/0/properties/state/const", keyword: "const", params: { allowedValue: "available" }, message: "must be equal to constant" };
                      if (vErrors === null) {
                        vErrors = [err22];
                      } else {
                        vErrors.push(err22);
                      }
                      errors++;
                    }
                    var valid19 = _errs53 === errors;
                  } else {
                    var valid19 = true;
                  }
                  if (valid19) {
                    if (data26.reason !== void 0) {
                      let data28 = data26.reason;
                      const _errs54 = errors;
                      if (!(data28 === "reported_by_adapter" || data28 === "derived_from_trace_context" || data28 === "legacy_v1_report")) {
                        const err23 = { instancePath: instancePath + "/model/reason", schemaPath: "#/$defs/field_availability/oneOf/0/properties/reason/enum", keyword: "enum", params: { allowedValues: schema107.oneOf[0].properties.reason.enum }, message: "must be equal to one of the allowed values" };
                        if (vErrors === null) {
                          vErrors = [err23];
                        } else {
                          vErrors.push(err23);
                        }
                        errors++;
                      }
                      var valid19 = _errs54 === errors;
                    } else {
                      var valid19 = true;
                    }
                  }
                }
                var _valid2 = _errs52 === errors;
                if (_valid2) {
                  valid18 = true;
                  passing2 = 0;
                  var props2 = {};
                  props2.state = true;
                  props2.reason = true;
                }
                const _errs55 = errors;
                if (data26 && typeof data26 == "object" && !Array.isArray(data26)) {
                  if (data26.state !== void 0) {
                    const _errs56 = errors;
                    if ("source_unavailable" !== data26.state) {
                      const err24 = { instancePath: instancePath + "/model/state", schemaPath: "#/$defs/field_availability/oneOf/1/properties/state/const", keyword: "const", params: { allowedValue: "source_unavailable" }, message: "must be equal to constant" };
                      if (vErrors === null) {
                        vErrors = [err24];
                      } else {
                        vErrors.push(err24);
                      }
                      errors++;
                    }
                    var valid20 = _errs56 === errors;
                  } else {
                    var valid20 = true;
                  }
                  if (valid20) {
                    if (data26.reason !== void 0) {
                      let data30 = data26.reason;
                      const _errs57 = errors;
                      if (!(data30 === "source_not_provided" || data30 === "not_evaluated" || data30 === "partial_token_metrics" || data30 === "historical_codex_source_not_lookup_eligible" || data30 === "codex_notify_turn_correlation_unavailable" || data30 === "ambiguous_trace_repository" || data30 === "legacy_v1_report")) {
                        const err25 = { instancePath: instancePath + "/model/reason", schemaPath: "#/$defs/field_availability/oneOf/1/properties/reason/enum", keyword: "enum", params: { allowedValues: schema107.oneOf[1].properties.reason.enum }, message: "must be equal to one of the allowed values" };
                        if (vErrors === null) {
                          vErrors = [err25];
                        } else {
                          vErrors.push(err25);
                        }
                        errors++;
                      }
                      var valid20 = _errs57 === errors;
                    } else {
                      var valid20 = true;
                    }
                  }
                }
                var _valid2 = _errs55 === errors;
                if (_valid2 && valid18) {
                  valid18 = false;
                  passing2 = [passing2, 1];
                } else {
                  if (_valid2) {
                    valid18 = true;
                    passing2 = 1;
                    if (props2 !== true) {
                      props2 = props2 || {};
                      props2.state = true;
                      props2.reason = true;
                    }
                  }
                  const _errs58 = errors;
                  if (data26 && typeof data26 == "object" && !Array.isArray(data26)) {
                    if (data26.state !== void 0) {
                      const _errs59 = errors;
                      if ("not_applicable" !== data26.state) {
                        const err26 = { instancePath: instancePath + "/model/state", schemaPath: "#/$defs/field_availability/oneOf/2/properties/state/const", keyword: "const", params: { allowedValue: "not_applicable" }, message: "must be equal to constant" };
                        if (vErrors === null) {
                          vErrors = [err26];
                        } else {
                          vErrors.push(err26);
                        }
                        errors++;
                      }
                      var valid21 = _errs59 === errors;
                    } else {
                      var valid21 = true;
                    }
                    if (valid21) {
                      if (data26.reason !== void 0) {
                        let data32 = data26.reason;
                        const _errs60 = errors;
                        if (!(data32 === "span_kind_not_model_backed" || data32 === "span_kind_has_no_latency" || data32 === "span_kind_has_no_token_usage" || data32 === "claude_private_lookup_not_supported" || data32 === "cursor_private_lookup_not_supported" || data32 === "codex_span_not_notify_derived" || data32 === "agent_private_lookup_not_supported")) {
                          const err27 = { instancePath: instancePath + "/model/reason", schemaPath: "#/$defs/field_availability/oneOf/2/properties/reason/enum", keyword: "enum", params: { allowedValues: schema107.oneOf[2].properties.reason.enum }, message: "must be equal to one of the allowed values" };
                          if (vErrors === null) {
                            vErrors = [err27];
                          } else {
                            vErrors.push(err27);
                          }
                          errors++;
                        }
                        var valid21 = _errs60 === errors;
                      } else {
                        var valid21 = true;
                      }
                    }
                  }
                  var _valid2 = _errs58 === errors;
                  if (_valid2 && valid18) {
                    valid18 = false;
                    passing2 = [passing2, 2];
                  } else {
                    if (_valid2) {
                      valid18 = true;
                      passing2 = 2;
                      if (props2 !== true) {
                        props2 = props2 || {};
                        props2.state = true;
                        props2.reason = true;
                      }
                    }
                    const _errs61 = errors;
                    if (data26 && typeof data26 == "object" && !Array.isArray(data26)) {
                      if (data26.state !== void 0) {
                        const _errs62 = errors;
                        if ("private_lookup" !== data26.state) {
                          const err28 = { instancePath: instancePath + "/model/state", schemaPath: "#/$defs/field_availability/oneOf/3/properties/state/const", keyword: "const", params: { allowedValue: "private_lookup" }, message: "must be equal to constant" };
                          if (vErrors === null) {
                            vErrors = [err28];
                          } else {
                            vErrors.push(err28);
                          }
                          errors++;
                        }
                        var valid22 = _errs62 === errors;
                      } else {
                        var valid22 = true;
                      }
                      if (valid22) {
                        if (data26.reason !== void 0) {
                          const _errs63 = errors;
                          if ("local_opt_in_lookup_required" !== data26.reason) {
                            const err29 = { instancePath: instancePath + "/model/reason", schemaPath: "#/$defs/field_availability/oneOf/3/properties/reason/const", keyword: "const", params: { allowedValue: "local_opt_in_lookup_required" }, message: "must be equal to constant" };
                            if (vErrors === null) {
                              vErrors = [err29];
                            } else {
                              vErrors.push(err29);
                            }
                            errors++;
                          }
                          var valid22 = _errs63 === errors;
                        } else {
                          var valid22 = true;
                        }
                      }
                    }
                    var _valid2 = _errs61 === errors;
                    if (_valid2 && valid18) {
                      valid18 = false;
                      passing2 = [passing2, 3];
                    } else {
                      if (_valid2) {
                        valid18 = true;
                        passing2 = 3;
                        if (props2 !== true) {
                          props2 = props2 || {};
                          props2.state = true;
                          props2.reason = true;
                        }
                      }
                      const _errs64 = errors;
                      if (data26 && typeof data26 == "object" && !Array.isArray(data26)) {
                        if (data26.state !== void 0) {
                          const _errs65 = errors;
                          if ("withheld" !== data26.state) {
                            const err30 = { instancePath: instancePath + "/model/state", schemaPath: "#/$defs/field_availability/oneOf/4/properties/state/const", keyword: "const", params: { allowedValue: "withheld" }, message: "must be equal to constant" };
                            if (vErrors === null) {
                              vErrors = [err30];
                            } else {
                              vErrors.push(err30);
                            }
                            errors++;
                          }
                          var valid23 = _errs65 === errors;
                        } else {
                          var valid23 = true;
                        }
                        if (valid23) {
                          if (data26.reason !== void 0) {
                            const _errs66 = errors;
                            if ("withheld_by_privacy_policy" !== data26.reason) {
                              const err31 = { instancePath: instancePath + "/model/reason", schemaPath: "#/$defs/field_availability/oneOf/4/properties/reason/const", keyword: "const", params: { allowedValue: "withheld_by_privacy_policy" }, message: "must be equal to constant" };
                              if (vErrors === null) {
                                vErrors = [err31];
                              } else {
                                vErrors.push(err31);
                              }
                              errors++;
                            }
                            var valid23 = _errs66 === errors;
                          } else {
                            var valid23 = true;
                          }
                        }
                      }
                      var _valid2 = _errs64 === errors;
                      if (_valid2 && valid18) {
                        valid18 = false;
                        passing2 = [passing2, 4];
                      } else {
                        if (_valid2) {
                          valid18 = true;
                          passing2 = 4;
                          if (props2 !== true) {
                            props2 = props2 || {};
                            props2.state = true;
                            props2.reason = true;
                          }
                        }
                      }
                    }
                  }
                }
                if (!valid18) {
                  const err32 = { instancePath: instancePath + "/model", schemaPath: "#/$defs/field_availability/oneOf", keyword: "oneOf", params: { passingSchemas: passing2 }, message: "must match exactly one schema in oneOf" };
                  if (vErrors === null) {
                    vErrors = [err32];
                  } else {
                    vErrors.push(err32);
                  }
                  errors++;
                  validate77.errors = vErrors;
                  return false;
                } else {
                  errors = _errs51;
                  if (vErrors !== null) {
                    if (_errs51) {
                      vErrors.length = _errs51;
                    } else {
                      vErrors = null;
                    }
                  }
                }
                if (errors === _errs49) {
                  if (data26 && typeof data26 == "object" && !Array.isArray(data26)) {
                    let missing3;
                    if (data26.state === void 0 && (missing3 = "state") || data26.reason === void 0 && (missing3 = "reason")) {
                      validate77.errors = [{ instancePath: instancePath + "/model", schemaPath: "#/$defs/field_availability/required", keyword: "required", params: { missingProperty: missing3 }, message: "must have required property '" + missing3 + "'" }];
                      return false;
                    } else {
                      const _errs67 = errors;
                      for (const key3 in data26) {
                        if (!(key3 === "state" || key3 === "reason")) {
                          validate77.errors = [{ instancePath: instancePath + "/model", schemaPath: "#/$defs/field_availability/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key3 }, message: "must NOT have additional properties" }];
                          return false;
                          break;
                        }
                      }
                      if (_errs67 === errors) {
                        if (data26.state !== void 0) {
                          let data37 = data26.state;
                          const _errs68 = errors;
                          if (!(data37 === "available" || data37 === "source_unavailable" || data37 === "withheld" || data37 === "not_applicable" || data37 === "private_lookup")) {
                            validate77.errors = [{ instancePath: instancePath + "/model/state", schemaPath: "#/$defs/field_availability/properties/state/enum", keyword: "enum", params: { allowedValues: schema107.properties.state.enum }, message: "must be equal to one of the allowed values" }];
                            return false;
                          }
                          var valid24 = _errs68 === errors;
                        } else {
                          var valid24 = true;
                        }
                        if (valid24) {
                          if (data26.reason !== void 0) {
                            let data38 = data26.reason;
                            const _errs69 = errors;
                            if (typeof data38 !== "string") {
                              validate77.errors = [{ instancePath: instancePath + "/model/reason", schemaPath: "#/$defs/field_availability/properties/reason/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                              return false;
                            }
                            if (!(data38 === "reported_by_adapter" || data38 === "derived_from_trace_context" || data38 === "legacy_v1_report" || data38 === "source_not_provided" || data38 === "not_evaluated" || data38 === "partial_token_metrics" || data38 === "historical_codex_source_not_lookup_eligible" || data38 === "codex_notify_turn_correlation_unavailable" || data38 === "ambiguous_trace_repository" || data38 === "span_kind_not_model_backed" || data38 === "span_kind_has_no_latency" || data38 === "span_kind_has_no_token_usage" || data38 === "claude_private_lookup_not_supported" || data38 === "cursor_private_lookup_not_supported" || data38 === "codex_span_not_notify_derived" || data38 === "agent_private_lookup_not_supported" || data38 === "local_opt_in_lookup_required" || data38 === "withheld_by_privacy_policy")) {
                              validate77.errors = [{ instancePath: instancePath + "/model/reason", schemaPath: "#/$defs/field_availability/properties/reason/enum", keyword: "enum", params: { allowedValues: schema107.properties.reason.enum }, message: "must be equal to one of the allowed values" }];
                              return false;
                            }
                            var valid24 = _errs69 === errors;
                          } else {
                            var valid24 = true;
                          }
                        }
                      }
                    }
                  } else {
                    validate77.errors = [{ instancePath: instancePath + "/model", schemaPath: "#/$defs/field_availability/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
                    return false;
                  }
                }
                var valid0 = _errs48 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.tokens !== void 0) {
                  let data39 = data.tokens;
                  const _errs71 = errors;
                  const _errs72 = errors;
                  const _errs74 = errors;
                  let valid26 = false;
                  let passing3 = null;
                  const _errs75 = errors;
                  if (data39 && typeof data39 == "object" && !Array.isArray(data39)) {
                    if (data39.state !== void 0) {
                      const _errs76 = errors;
                      if ("available" !== data39.state) {
                        const err33 = { instancePath: instancePath + "/tokens/state", schemaPath: "#/$defs/field_availability/oneOf/0/properties/state/const", keyword: "const", params: { allowedValue: "available" }, message: "must be equal to constant" };
                        if (vErrors === null) {
                          vErrors = [err33];
                        } else {
                          vErrors.push(err33);
                        }
                        errors++;
                      }
                      var valid27 = _errs76 === errors;
                    } else {
                      var valid27 = true;
                    }
                    if (valid27) {
                      if (data39.reason !== void 0) {
                        let data41 = data39.reason;
                        const _errs77 = errors;
                        if (!(data41 === "reported_by_adapter" || data41 === "derived_from_trace_context" || data41 === "legacy_v1_report")) {
                          const err34 = { instancePath: instancePath + "/tokens/reason", schemaPath: "#/$defs/field_availability/oneOf/0/properties/reason/enum", keyword: "enum", params: { allowedValues: schema107.oneOf[0].properties.reason.enum }, message: "must be equal to one of the allowed values" };
                          if (vErrors === null) {
                            vErrors = [err34];
                          } else {
                            vErrors.push(err34);
                          }
                          errors++;
                        }
                        var valid27 = _errs77 === errors;
                      } else {
                        var valid27 = true;
                      }
                    }
                  }
                  var _valid3 = _errs75 === errors;
                  if (_valid3) {
                    valid26 = true;
                    passing3 = 0;
                    var props3 = {};
                    props3.state = true;
                    props3.reason = true;
                  }
                  const _errs78 = errors;
                  if (data39 && typeof data39 == "object" && !Array.isArray(data39)) {
                    if (data39.state !== void 0) {
                      const _errs79 = errors;
                      if ("source_unavailable" !== data39.state) {
                        const err35 = { instancePath: instancePath + "/tokens/state", schemaPath: "#/$defs/field_availability/oneOf/1/properties/state/const", keyword: "const", params: { allowedValue: "source_unavailable" }, message: "must be equal to constant" };
                        if (vErrors === null) {
                          vErrors = [err35];
                        } else {
                          vErrors.push(err35);
                        }
                        errors++;
                      }
                      var valid28 = _errs79 === errors;
                    } else {
                      var valid28 = true;
                    }
                    if (valid28) {
                      if (data39.reason !== void 0) {
                        let data43 = data39.reason;
                        const _errs80 = errors;
                        if (!(data43 === "source_not_provided" || data43 === "not_evaluated" || data43 === "partial_token_metrics" || data43 === "historical_codex_source_not_lookup_eligible" || data43 === "codex_notify_turn_correlation_unavailable" || data43 === "ambiguous_trace_repository" || data43 === "legacy_v1_report")) {
                          const err36 = { instancePath: instancePath + "/tokens/reason", schemaPath: "#/$defs/field_availability/oneOf/1/properties/reason/enum", keyword: "enum", params: { allowedValues: schema107.oneOf[1].properties.reason.enum }, message: "must be equal to one of the allowed values" };
                          if (vErrors === null) {
                            vErrors = [err36];
                          } else {
                            vErrors.push(err36);
                          }
                          errors++;
                        }
                        var valid28 = _errs80 === errors;
                      } else {
                        var valid28 = true;
                      }
                    }
                  }
                  var _valid3 = _errs78 === errors;
                  if (_valid3 && valid26) {
                    valid26 = false;
                    passing3 = [passing3, 1];
                  } else {
                    if (_valid3) {
                      valid26 = true;
                      passing3 = 1;
                      if (props3 !== true) {
                        props3 = props3 || {};
                        props3.state = true;
                        props3.reason = true;
                      }
                    }
                    const _errs81 = errors;
                    if (data39 && typeof data39 == "object" && !Array.isArray(data39)) {
                      if (data39.state !== void 0) {
                        const _errs82 = errors;
                        if ("not_applicable" !== data39.state) {
                          const err37 = { instancePath: instancePath + "/tokens/state", schemaPath: "#/$defs/field_availability/oneOf/2/properties/state/const", keyword: "const", params: { allowedValue: "not_applicable" }, message: "must be equal to constant" };
                          if (vErrors === null) {
                            vErrors = [err37];
                          } else {
                            vErrors.push(err37);
                          }
                          errors++;
                        }
                        var valid29 = _errs82 === errors;
                      } else {
                        var valid29 = true;
                      }
                      if (valid29) {
                        if (data39.reason !== void 0) {
                          let data45 = data39.reason;
                          const _errs83 = errors;
                          if (!(data45 === "span_kind_not_model_backed" || data45 === "span_kind_has_no_latency" || data45 === "span_kind_has_no_token_usage" || data45 === "claude_private_lookup_not_supported" || data45 === "cursor_private_lookup_not_supported" || data45 === "codex_span_not_notify_derived" || data45 === "agent_private_lookup_not_supported")) {
                            const err38 = { instancePath: instancePath + "/tokens/reason", schemaPath: "#/$defs/field_availability/oneOf/2/properties/reason/enum", keyword: "enum", params: { allowedValues: schema107.oneOf[2].properties.reason.enum }, message: "must be equal to one of the allowed values" };
                            if (vErrors === null) {
                              vErrors = [err38];
                            } else {
                              vErrors.push(err38);
                            }
                            errors++;
                          }
                          var valid29 = _errs83 === errors;
                        } else {
                          var valid29 = true;
                        }
                      }
                    }
                    var _valid3 = _errs81 === errors;
                    if (_valid3 && valid26) {
                      valid26 = false;
                      passing3 = [passing3, 2];
                    } else {
                      if (_valid3) {
                        valid26 = true;
                        passing3 = 2;
                        if (props3 !== true) {
                          props3 = props3 || {};
                          props3.state = true;
                          props3.reason = true;
                        }
                      }
                      const _errs84 = errors;
                      if (data39 && typeof data39 == "object" && !Array.isArray(data39)) {
                        if (data39.state !== void 0) {
                          const _errs85 = errors;
                          if ("private_lookup" !== data39.state) {
                            const err39 = { instancePath: instancePath + "/tokens/state", schemaPath: "#/$defs/field_availability/oneOf/3/properties/state/const", keyword: "const", params: { allowedValue: "private_lookup" }, message: "must be equal to constant" };
                            if (vErrors === null) {
                              vErrors = [err39];
                            } else {
                              vErrors.push(err39);
                            }
                            errors++;
                          }
                          var valid30 = _errs85 === errors;
                        } else {
                          var valid30 = true;
                        }
                        if (valid30) {
                          if (data39.reason !== void 0) {
                            const _errs86 = errors;
                            if ("local_opt_in_lookup_required" !== data39.reason) {
                              const err40 = { instancePath: instancePath + "/tokens/reason", schemaPath: "#/$defs/field_availability/oneOf/3/properties/reason/const", keyword: "const", params: { allowedValue: "local_opt_in_lookup_required" }, message: "must be equal to constant" };
                              if (vErrors === null) {
                                vErrors = [err40];
                              } else {
                                vErrors.push(err40);
                              }
                              errors++;
                            }
                            var valid30 = _errs86 === errors;
                          } else {
                            var valid30 = true;
                          }
                        }
                      }
                      var _valid3 = _errs84 === errors;
                      if (_valid3 && valid26) {
                        valid26 = false;
                        passing3 = [passing3, 3];
                      } else {
                        if (_valid3) {
                          valid26 = true;
                          passing3 = 3;
                          if (props3 !== true) {
                            props3 = props3 || {};
                            props3.state = true;
                            props3.reason = true;
                          }
                        }
                        const _errs87 = errors;
                        if (data39 && typeof data39 == "object" && !Array.isArray(data39)) {
                          if (data39.state !== void 0) {
                            const _errs88 = errors;
                            if ("withheld" !== data39.state) {
                              const err41 = { instancePath: instancePath + "/tokens/state", schemaPath: "#/$defs/field_availability/oneOf/4/properties/state/const", keyword: "const", params: { allowedValue: "withheld" }, message: "must be equal to constant" };
                              if (vErrors === null) {
                                vErrors = [err41];
                              } else {
                                vErrors.push(err41);
                              }
                              errors++;
                            }
                            var valid31 = _errs88 === errors;
                          } else {
                            var valid31 = true;
                          }
                          if (valid31) {
                            if (data39.reason !== void 0) {
                              const _errs89 = errors;
                              if ("withheld_by_privacy_policy" !== data39.reason) {
                                const err42 = { instancePath: instancePath + "/tokens/reason", schemaPath: "#/$defs/field_availability/oneOf/4/properties/reason/const", keyword: "const", params: { allowedValue: "withheld_by_privacy_policy" }, message: "must be equal to constant" };
                                if (vErrors === null) {
                                  vErrors = [err42];
                                } else {
                                  vErrors.push(err42);
                                }
                                errors++;
                              }
                              var valid31 = _errs89 === errors;
                            } else {
                              var valid31 = true;
                            }
                          }
                        }
                        var _valid3 = _errs87 === errors;
                        if (_valid3 && valid26) {
                          valid26 = false;
                          passing3 = [passing3, 4];
                        } else {
                          if (_valid3) {
                            valid26 = true;
                            passing3 = 4;
                            if (props3 !== true) {
                              props3 = props3 || {};
                              props3.state = true;
                              props3.reason = true;
                            }
                          }
                        }
                      }
                    }
                  }
                  if (!valid26) {
                    const err43 = { instancePath: instancePath + "/tokens", schemaPath: "#/$defs/field_availability/oneOf", keyword: "oneOf", params: { passingSchemas: passing3 }, message: "must match exactly one schema in oneOf" };
                    if (vErrors === null) {
                      vErrors = [err43];
                    } else {
                      vErrors.push(err43);
                    }
                    errors++;
                    validate77.errors = vErrors;
                    return false;
                  } else {
                    errors = _errs74;
                    if (vErrors !== null) {
                      if (_errs74) {
                        vErrors.length = _errs74;
                      } else {
                        vErrors = null;
                      }
                    }
                  }
                  if (errors === _errs72) {
                    if (data39 && typeof data39 == "object" && !Array.isArray(data39)) {
                      let missing4;
                      if (data39.state === void 0 && (missing4 = "state") || data39.reason === void 0 && (missing4 = "reason")) {
                        validate77.errors = [{ instancePath: instancePath + "/tokens", schemaPath: "#/$defs/field_availability/required", keyword: "required", params: { missingProperty: missing4 }, message: "must have required property '" + missing4 + "'" }];
                        return false;
                      } else {
                        const _errs90 = errors;
                        for (const key4 in data39) {
                          if (!(key4 === "state" || key4 === "reason")) {
                            validate77.errors = [{ instancePath: instancePath + "/tokens", schemaPath: "#/$defs/field_availability/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key4 }, message: "must NOT have additional properties" }];
                            return false;
                            break;
                          }
                        }
                        if (_errs90 === errors) {
                          if (data39.state !== void 0) {
                            let data50 = data39.state;
                            const _errs91 = errors;
                            if (!(data50 === "available" || data50 === "source_unavailable" || data50 === "withheld" || data50 === "not_applicable" || data50 === "private_lookup")) {
                              validate77.errors = [{ instancePath: instancePath + "/tokens/state", schemaPath: "#/$defs/field_availability/properties/state/enum", keyword: "enum", params: { allowedValues: schema107.properties.state.enum }, message: "must be equal to one of the allowed values" }];
                              return false;
                            }
                            var valid32 = _errs91 === errors;
                          } else {
                            var valid32 = true;
                          }
                          if (valid32) {
                            if (data39.reason !== void 0) {
                              let data51 = data39.reason;
                              const _errs92 = errors;
                              if (typeof data51 !== "string") {
                                validate77.errors = [{ instancePath: instancePath + "/tokens/reason", schemaPath: "#/$defs/field_availability/properties/reason/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                return false;
                              }
                              if (!(data51 === "reported_by_adapter" || data51 === "derived_from_trace_context" || data51 === "legacy_v1_report" || data51 === "source_not_provided" || data51 === "not_evaluated" || data51 === "partial_token_metrics" || data51 === "historical_codex_source_not_lookup_eligible" || data51 === "codex_notify_turn_correlation_unavailable" || data51 === "ambiguous_trace_repository" || data51 === "span_kind_not_model_backed" || data51 === "span_kind_has_no_latency" || data51 === "span_kind_has_no_token_usage" || data51 === "claude_private_lookup_not_supported" || data51 === "cursor_private_lookup_not_supported" || data51 === "codex_span_not_notify_derived" || data51 === "agent_private_lookup_not_supported" || data51 === "local_opt_in_lookup_required" || data51 === "withheld_by_privacy_policy")) {
                                validate77.errors = [{ instancePath: instancePath + "/tokens/reason", schemaPath: "#/$defs/field_availability/properties/reason/enum", keyword: "enum", params: { allowedValues: schema107.properties.reason.enum }, message: "must be equal to one of the allowed values" }];
                                return false;
                              }
                              var valid32 = _errs92 === errors;
                            } else {
                              var valid32 = true;
                            }
                          }
                        }
                      }
                    } else {
                      validate77.errors = [{ instancePath: instancePath + "/tokens", schemaPath: "#/$defs/field_availability/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
                      return false;
                    }
                  }
                  var valid0 = _errs71 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.latency !== void 0) {
                    let data52 = data.latency;
                    const _errs94 = errors;
                    const _errs95 = errors;
                    const _errs97 = errors;
                    let valid34 = false;
                    let passing4 = null;
                    const _errs98 = errors;
                    if (data52 && typeof data52 == "object" && !Array.isArray(data52)) {
                      if (data52.state !== void 0) {
                        const _errs99 = errors;
                        if ("available" !== data52.state) {
                          const err44 = { instancePath: instancePath + "/latency/state", schemaPath: "#/$defs/field_availability/oneOf/0/properties/state/const", keyword: "const", params: { allowedValue: "available" }, message: "must be equal to constant" };
                          if (vErrors === null) {
                            vErrors = [err44];
                          } else {
                            vErrors.push(err44);
                          }
                          errors++;
                        }
                        var valid35 = _errs99 === errors;
                      } else {
                        var valid35 = true;
                      }
                      if (valid35) {
                        if (data52.reason !== void 0) {
                          let data54 = data52.reason;
                          const _errs100 = errors;
                          if (!(data54 === "reported_by_adapter" || data54 === "derived_from_trace_context" || data54 === "legacy_v1_report")) {
                            const err45 = { instancePath: instancePath + "/latency/reason", schemaPath: "#/$defs/field_availability/oneOf/0/properties/reason/enum", keyword: "enum", params: { allowedValues: schema107.oneOf[0].properties.reason.enum }, message: "must be equal to one of the allowed values" };
                            if (vErrors === null) {
                              vErrors = [err45];
                            } else {
                              vErrors.push(err45);
                            }
                            errors++;
                          }
                          var valid35 = _errs100 === errors;
                        } else {
                          var valid35 = true;
                        }
                      }
                    }
                    var _valid4 = _errs98 === errors;
                    if (_valid4) {
                      valid34 = true;
                      passing4 = 0;
                      var props4 = {};
                      props4.state = true;
                      props4.reason = true;
                    }
                    const _errs101 = errors;
                    if (data52 && typeof data52 == "object" && !Array.isArray(data52)) {
                      if (data52.state !== void 0) {
                        const _errs102 = errors;
                        if ("source_unavailable" !== data52.state) {
                          const err46 = { instancePath: instancePath + "/latency/state", schemaPath: "#/$defs/field_availability/oneOf/1/properties/state/const", keyword: "const", params: { allowedValue: "source_unavailable" }, message: "must be equal to constant" };
                          if (vErrors === null) {
                            vErrors = [err46];
                          } else {
                            vErrors.push(err46);
                          }
                          errors++;
                        }
                        var valid36 = _errs102 === errors;
                      } else {
                        var valid36 = true;
                      }
                      if (valid36) {
                        if (data52.reason !== void 0) {
                          let data56 = data52.reason;
                          const _errs103 = errors;
                          if (!(data56 === "source_not_provided" || data56 === "not_evaluated" || data56 === "partial_token_metrics" || data56 === "historical_codex_source_not_lookup_eligible" || data56 === "codex_notify_turn_correlation_unavailable" || data56 === "ambiguous_trace_repository" || data56 === "legacy_v1_report")) {
                            const err47 = { instancePath: instancePath + "/latency/reason", schemaPath: "#/$defs/field_availability/oneOf/1/properties/reason/enum", keyword: "enum", params: { allowedValues: schema107.oneOf[1].properties.reason.enum }, message: "must be equal to one of the allowed values" };
                            if (vErrors === null) {
                              vErrors = [err47];
                            } else {
                              vErrors.push(err47);
                            }
                            errors++;
                          }
                          var valid36 = _errs103 === errors;
                        } else {
                          var valid36 = true;
                        }
                      }
                    }
                    var _valid4 = _errs101 === errors;
                    if (_valid4 && valid34) {
                      valid34 = false;
                      passing4 = [passing4, 1];
                    } else {
                      if (_valid4) {
                        valid34 = true;
                        passing4 = 1;
                        if (props4 !== true) {
                          props4 = props4 || {};
                          props4.state = true;
                          props4.reason = true;
                        }
                      }
                      const _errs104 = errors;
                      if (data52 && typeof data52 == "object" && !Array.isArray(data52)) {
                        if (data52.state !== void 0) {
                          const _errs105 = errors;
                          if ("not_applicable" !== data52.state) {
                            const err48 = { instancePath: instancePath + "/latency/state", schemaPath: "#/$defs/field_availability/oneOf/2/properties/state/const", keyword: "const", params: { allowedValue: "not_applicable" }, message: "must be equal to constant" };
                            if (vErrors === null) {
                              vErrors = [err48];
                            } else {
                              vErrors.push(err48);
                            }
                            errors++;
                          }
                          var valid37 = _errs105 === errors;
                        } else {
                          var valid37 = true;
                        }
                        if (valid37) {
                          if (data52.reason !== void 0) {
                            let data58 = data52.reason;
                            const _errs106 = errors;
                            if (!(data58 === "span_kind_not_model_backed" || data58 === "span_kind_has_no_latency" || data58 === "span_kind_has_no_token_usage" || data58 === "claude_private_lookup_not_supported" || data58 === "cursor_private_lookup_not_supported" || data58 === "codex_span_not_notify_derived" || data58 === "agent_private_lookup_not_supported")) {
                              const err49 = { instancePath: instancePath + "/latency/reason", schemaPath: "#/$defs/field_availability/oneOf/2/properties/reason/enum", keyword: "enum", params: { allowedValues: schema107.oneOf[2].properties.reason.enum }, message: "must be equal to one of the allowed values" };
                              if (vErrors === null) {
                                vErrors = [err49];
                              } else {
                                vErrors.push(err49);
                              }
                              errors++;
                            }
                            var valid37 = _errs106 === errors;
                          } else {
                            var valid37 = true;
                          }
                        }
                      }
                      var _valid4 = _errs104 === errors;
                      if (_valid4 && valid34) {
                        valid34 = false;
                        passing4 = [passing4, 2];
                      } else {
                        if (_valid4) {
                          valid34 = true;
                          passing4 = 2;
                          if (props4 !== true) {
                            props4 = props4 || {};
                            props4.state = true;
                            props4.reason = true;
                          }
                        }
                        const _errs107 = errors;
                        if (data52 && typeof data52 == "object" && !Array.isArray(data52)) {
                          if (data52.state !== void 0) {
                            const _errs108 = errors;
                            if ("private_lookup" !== data52.state) {
                              const err50 = { instancePath: instancePath + "/latency/state", schemaPath: "#/$defs/field_availability/oneOf/3/properties/state/const", keyword: "const", params: { allowedValue: "private_lookup" }, message: "must be equal to constant" };
                              if (vErrors === null) {
                                vErrors = [err50];
                              } else {
                                vErrors.push(err50);
                              }
                              errors++;
                            }
                            var valid38 = _errs108 === errors;
                          } else {
                            var valid38 = true;
                          }
                          if (valid38) {
                            if (data52.reason !== void 0) {
                              const _errs109 = errors;
                              if ("local_opt_in_lookup_required" !== data52.reason) {
                                const err51 = { instancePath: instancePath + "/latency/reason", schemaPath: "#/$defs/field_availability/oneOf/3/properties/reason/const", keyword: "const", params: { allowedValue: "local_opt_in_lookup_required" }, message: "must be equal to constant" };
                                if (vErrors === null) {
                                  vErrors = [err51];
                                } else {
                                  vErrors.push(err51);
                                }
                                errors++;
                              }
                              var valid38 = _errs109 === errors;
                            } else {
                              var valid38 = true;
                            }
                          }
                        }
                        var _valid4 = _errs107 === errors;
                        if (_valid4 && valid34) {
                          valid34 = false;
                          passing4 = [passing4, 3];
                        } else {
                          if (_valid4) {
                            valid34 = true;
                            passing4 = 3;
                            if (props4 !== true) {
                              props4 = props4 || {};
                              props4.state = true;
                              props4.reason = true;
                            }
                          }
                          const _errs110 = errors;
                          if (data52 && typeof data52 == "object" && !Array.isArray(data52)) {
                            if (data52.state !== void 0) {
                              const _errs111 = errors;
                              if ("withheld" !== data52.state) {
                                const err52 = { instancePath: instancePath + "/latency/state", schemaPath: "#/$defs/field_availability/oneOf/4/properties/state/const", keyword: "const", params: { allowedValue: "withheld" }, message: "must be equal to constant" };
                                if (vErrors === null) {
                                  vErrors = [err52];
                                } else {
                                  vErrors.push(err52);
                                }
                                errors++;
                              }
                              var valid39 = _errs111 === errors;
                            } else {
                              var valid39 = true;
                            }
                            if (valid39) {
                              if (data52.reason !== void 0) {
                                const _errs112 = errors;
                                if ("withheld_by_privacy_policy" !== data52.reason) {
                                  const err53 = { instancePath: instancePath + "/latency/reason", schemaPath: "#/$defs/field_availability/oneOf/4/properties/reason/const", keyword: "const", params: { allowedValue: "withheld_by_privacy_policy" }, message: "must be equal to constant" };
                                  if (vErrors === null) {
                                    vErrors = [err53];
                                  } else {
                                    vErrors.push(err53);
                                  }
                                  errors++;
                                }
                                var valid39 = _errs112 === errors;
                              } else {
                                var valid39 = true;
                              }
                            }
                          }
                          var _valid4 = _errs110 === errors;
                          if (_valid4 && valid34) {
                            valid34 = false;
                            passing4 = [passing4, 4];
                          } else {
                            if (_valid4) {
                              valid34 = true;
                              passing4 = 4;
                              if (props4 !== true) {
                                props4 = props4 || {};
                                props4.state = true;
                                props4.reason = true;
                              }
                            }
                          }
                        }
                      }
                    }
                    if (!valid34) {
                      const err54 = { instancePath: instancePath + "/latency", schemaPath: "#/$defs/field_availability/oneOf", keyword: "oneOf", params: { passingSchemas: passing4 }, message: "must match exactly one schema in oneOf" };
                      if (vErrors === null) {
                        vErrors = [err54];
                      } else {
                        vErrors.push(err54);
                      }
                      errors++;
                      validate77.errors = vErrors;
                      return false;
                    } else {
                      errors = _errs97;
                      if (vErrors !== null) {
                        if (_errs97) {
                          vErrors.length = _errs97;
                        } else {
                          vErrors = null;
                        }
                      }
                    }
                    if (errors === _errs95) {
                      if (data52 && typeof data52 == "object" && !Array.isArray(data52)) {
                        let missing5;
                        if (data52.state === void 0 && (missing5 = "state") || data52.reason === void 0 && (missing5 = "reason")) {
                          validate77.errors = [{ instancePath: instancePath + "/latency", schemaPath: "#/$defs/field_availability/required", keyword: "required", params: { missingProperty: missing5 }, message: "must have required property '" + missing5 + "'" }];
                          return false;
                        } else {
                          const _errs113 = errors;
                          for (const key5 in data52) {
                            if (!(key5 === "state" || key5 === "reason")) {
                              validate77.errors = [{ instancePath: instancePath + "/latency", schemaPath: "#/$defs/field_availability/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key5 }, message: "must NOT have additional properties" }];
                              return false;
                              break;
                            }
                          }
                          if (_errs113 === errors) {
                            if (data52.state !== void 0) {
                              let data63 = data52.state;
                              const _errs114 = errors;
                              if (!(data63 === "available" || data63 === "source_unavailable" || data63 === "withheld" || data63 === "not_applicable" || data63 === "private_lookup")) {
                                validate77.errors = [{ instancePath: instancePath + "/latency/state", schemaPath: "#/$defs/field_availability/properties/state/enum", keyword: "enum", params: { allowedValues: schema107.properties.state.enum }, message: "must be equal to one of the allowed values" }];
                                return false;
                              }
                              var valid40 = _errs114 === errors;
                            } else {
                              var valid40 = true;
                            }
                            if (valid40) {
                              if (data52.reason !== void 0) {
                                let data64 = data52.reason;
                                const _errs115 = errors;
                                if (typeof data64 !== "string") {
                                  validate77.errors = [{ instancePath: instancePath + "/latency/reason", schemaPath: "#/$defs/field_availability/properties/reason/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                  return false;
                                }
                                if (!(data64 === "reported_by_adapter" || data64 === "derived_from_trace_context" || data64 === "legacy_v1_report" || data64 === "source_not_provided" || data64 === "not_evaluated" || data64 === "partial_token_metrics" || data64 === "historical_codex_source_not_lookup_eligible" || data64 === "codex_notify_turn_correlation_unavailable" || data64 === "ambiguous_trace_repository" || data64 === "span_kind_not_model_backed" || data64 === "span_kind_has_no_latency" || data64 === "span_kind_has_no_token_usage" || data64 === "claude_private_lookup_not_supported" || data64 === "cursor_private_lookup_not_supported" || data64 === "codex_span_not_notify_derived" || data64 === "agent_private_lookup_not_supported" || data64 === "local_opt_in_lookup_required" || data64 === "withheld_by_privacy_policy")) {
                                  validate77.errors = [{ instancePath: instancePath + "/latency/reason", schemaPath: "#/$defs/field_availability/properties/reason/enum", keyword: "enum", params: { allowedValues: schema107.properties.reason.enum }, message: "must be equal to one of the allowed values" }];
                                  return false;
                                }
                                var valid40 = _errs115 === errors;
                              } else {
                                var valid40 = true;
                              }
                            }
                          }
                        }
                      } else {
                        validate77.errors = [{ instancePath: instancePath + "/latency", schemaPath: "#/$defs/field_availability/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
                        return false;
                      }
                    }
                    var valid0 = _errs94 === errors;
                  } else {
                    var valid0 = true;
                  }
                  if (valid0) {
                    if (data.sourceLocation !== void 0) {
                      let data65 = data.sourceLocation;
                      const _errs117 = errors;
                      const _errs118 = errors;
                      const _errs120 = errors;
                      let valid42 = false;
                      let passing5 = null;
                      const _errs121 = errors;
                      if (data65 && typeof data65 == "object" && !Array.isArray(data65)) {
                        if (data65.state !== void 0) {
                          const _errs122 = errors;
                          if ("available" !== data65.state) {
                            const err55 = { instancePath: instancePath + "/sourceLocation/state", schemaPath: "#/$defs/field_availability/oneOf/0/properties/state/const", keyword: "const", params: { allowedValue: "available" }, message: "must be equal to constant" };
                            if (vErrors === null) {
                              vErrors = [err55];
                            } else {
                              vErrors.push(err55);
                            }
                            errors++;
                          }
                          var valid43 = _errs122 === errors;
                        } else {
                          var valid43 = true;
                        }
                        if (valid43) {
                          if (data65.reason !== void 0) {
                            let data67 = data65.reason;
                            const _errs123 = errors;
                            if (!(data67 === "reported_by_adapter" || data67 === "derived_from_trace_context" || data67 === "legacy_v1_report")) {
                              const err56 = { instancePath: instancePath + "/sourceLocation/reason", schemaPath: "#/$defs/field_availability/oneOf/0/properties/reason/enum", keyword: "enum", params: { allowedValues: schema107.oneOf[0].properties.reason.enum }, message: "must be equal to one of the allowed values" };
                              if (vErrors === null) {
                                vErrors = [err56];
                              } else {
                                vErrors.push(err56);
                              }
                              errors++;
                            }
                            var valid43 = _errs123 === errors;
                          } else {
                            var valid43 = true;
                          }
                        }
                      }
                      var _valid5 = _errs121 === errors;
                      if (_valid5) {
                        valid42 = true;
                        passing5 = 0;
                        var props5 = {};
                        props5.state = true;
                        props5.reason = true;
                      }
                      const _errs124 = errors;
                      if (data65 && typeof data65 == "object" && !Array.isArray(data65)) {
                        if (data65.state !== void 0) {
                          const _errs125 = errors;
                          if ("source_unavailable" !== data65.state) {
                            const err57 = { instancePath: instancePath + "/sourceLocation/state", schemaPath: "#/$defs/field_availability/oneOf/1/properties/state/const", keyword: "const", params: { allowedValue: "source_unavailable" }, message: "must be equal to constant" };
                            if (vErrors === null) {
                              vErrors = [err57];
                            } else {
                              vErrors.push(err57);
                            }
                            errors++;
                          }
                          var valid44 = _errs125 === errors;
                        } else {
                          var valid44 = true;
                        }
                        if (valid44) {
                          if (data65.reason !== void 0) {
                            let data69 = data65.reason;
                            const _errs126 = errors;
                            if (!(data69 === "source_not_provided" || data69 === "not_evaluated" || data69 === "partial_token_metrics" || data69 === "historical_codex_source_not_lookup_eligible" || data69 === "codex_notify_turn_correlation_unavailable" || data69 === "ambiguous_trace_repository" || data69 === "legacy_v1_report")) {
                              const err58 = { instancePath: instancePath + "/sourceLocation/reason", schemaPath: "#/$defs/field_availability/oneOf/1/properties/reason/enum", keyword: "enum", params: { allowedValues: schema107.oneOf[1].properties.reason.enum }, message: "must be equal to one of the allowed values" };
                              if (vErrors === null) {
                                vErrors = [err58];
                              } else {
                                vErrors.push(err58);
                              }
                              errors++;
                            }
                            var valid44 = _errs126 === errors;
                          } else {
                            var valid44 = true;
                          }
                        }
                      }
                      var _valid5 = _errs124 === errors;
                      if (_valid5 && valid42) {
                        valid42 = false;
                        passing5 = [passing5, 1];
                      } else {
                        if (_valid5) {
                          valid42 = true;
                          passing5 = 1;
                          if (props5 !== true) {
                            props5 = props5 || {};
                            props5.state = true;
                            props5.reason = true;
                          }
                        }
                        const _errs127 = errors;
                        if (data65 && typeof data65 == "object" && !Array.isArray(data65)) {
                          if (data65.state !== void 0) {
                            const _errs128 = errors;
                            if ("not_applicable" !== data65.state) {
                              const err59 = { instancePath: instancePath + "/sourceLocation/state", schemaPath: "#/$defs/field_availability/oneOf/2/properties/state/const", keyword: "const", params: { allowedValue: "not_applicable" }, message: "must be equal to constant" };
                              if (vErrors === null) {
                                vErrors = [err59];
                              } else {
                                vErrors.push(err59);
                              }
                              errors++;
                            }
                            var valid45 = _errs128 === errors;
                          } else {
                            var valid45 = true;
                          }
                          if (valid45) {
                            if (data65.reason !== void 0) {
                              let data71 = data65.reason;
                              const _errs129 = errors;
                              if (!(data71 === "span_kind_not_model_backed" || data71 === "span_kind_has_no_latency" || data71 === "span_kind_has_no_token_usage" || data71 === "claude_private_lookup_not_supported" || data71 === "cursor_private_lookup_not_supported" || data71 === "codex_span_not_notify_derived" || data71 === "agent_private_lookup_not_supported")) {
                                const err60 = { instancePath: instancePath + "/sourceLocation/reason", schemaPath: "#/$defs/field_availability/oneOf/2/properties/reason/enum", keyword: "enum", params: { allowedValues: schema107.oneOf[2].properties.reason.enum }, message: "must be equal to one of the allowed values" };
                                if (vErrors === null) {
                                  vErrors = [err60];
                                } else {
                                  vErrors.push(err60);
                                }
                                errors++;
                              }
                              var valid45 = _errs129 === errors;
                            } else {
                              var valid45 = true;
                            }
                          }
                        }
                        var _valid5 = _errs127 === errors;
                        if (_valid5 && valid42) {
                          valid42 = false;
                          passing5 = [passing5, 2];
                        } else {
                          if (_valid5) {
                            valid42 = true;
                            passing5 = 2;
                            if (props5 !== true) {
                              props5 = props5 || {};
                              props5.state = true;
                              props5.reason = true;
                            }
                          }
                          const _errs130 = errors;
                          if (data65 && typeof data65 == "object" && !Array.isArray(data65)) {
                            if (data65.state !== void 0) {
                              const _errs131 = errors;
                              if ("private_lookup" !== data65.state) {
                                const err61 = { instancePath: instancePath + "/sourceLocation/state", schemaPath: "#/$defs/field_availability/oneOf/3/properties/state/const", keyword: "const", params: { allowedValue: "private_lookup" }, message: "must be equal to constant" };
                                if (vErrors === null) {
                                  vErrors = [err61];
                                } else {
                                  vErrors.push(err61);
                                }
                                errors++;
                              }
                              var valid46 = _errs131 === errors;
                            } else {
                              var valid46 = true;
                            }
                            if (valid46) {
                              if (data65.reason !== void 0) {
                                const _errs132 = errors;
                                if ("local_opt_in_lookup_required" !== data65.reason) {
                                  const err62 = { instancePath: instancePath + "/sourceLocation/reason", schemaPath: "#/$defs/field_availability/oneOf/3/properties/reason/const", keyword: "const", params: { allowedValue: "local_opt_in_lookup_required" }, message: "must be equal to constant" };
                                  if (vErrors === null) {
                                    vErrors = [err62];
                                  } else {
                                    vErrors.push(err62);
                                  }
                                  errors++;
                                }
                                var valid46 = _errs132 === errors;
                              } else {
                                var valid46 = true;
                              }
                            }
                          }
                          var _valid5 = _errs130 === errors;
                          if (_valid5 && valid42) {
                            valid42 = false;
                            passing5 = [passing5, 3];
                          } else {
                            if (_valid5) {
                              valid42 = true;
                              passing5 = 3;
                              if (props5 !== true) {
                                props5 = props5 || {};
                                props5.state = true;
                                props5.reason = true;
                              }
                            }
                            const _errs133 = errors;
                            if (data65 && typeof data65 == "object" && !Array.isArray(data65)) {
                              if (data65.state !== void 0) {
                                const _errs134 = errors;
                                if ("withheld" !== data65.state) {
                                  const err63 = { instancePath: instancePath + "/sourceLocation/state", schemaPath: "#/$defs/field_availability/oneOf/4/properties/state/const", keyword: "const", params: { allowedValue: "withheld" }, message: "must be equal to constant" };
                                  if (vErrors === null) {
                                    vErrors = [err63];
                                  } else {
                                    vErrors.push(err63);
                                  }
                                  errors++;
                                }
                                var valid47 = _errs134 === errors;
                              } else {
                                var valid47 = true;
                              }
                              if (valid47) {
                                if (data65.reason !== void 0) {
                                  const _errs135 = errors;
                                  if ("withheld_by_privacy_policy" !== data65.reason) {
                                    const err64 = { instancePath: instancePath + "/sourceLocation/reason", schemaPath: "#/$defs/field_availability/oneOf/4/properties/reason/const", keyword: "const", params: { allowedValue: "withheld_by_privacy_policy" }, message: "must be equal to constant" };
                                    if (vErrors === null) {
                                      vErrors = [err64];
                                    } else {
                                      vErrors.push(err64);
                                    }
                                    errors++;
                                  }
                                  var valid47 = _errs135 === errors;
                                } else {
                                  var valid47 = true;
                                }
                              }
                            }
                            var _valid5 = _errs133 === errors;
                            if (_valid5 && valid42) {
                              valid42 = false;
                              passing5 = [passing5, 4];
                            } else {
                              if (_valid5) {
                                valid42 = true;
                                passing5 = 4;
                                if (props5 !== true) {
                                  props5 = props5 || {};
                                  props5.state = true;
                                  props5.reason = true;
                                }
                              }
                            }
                          }
                        }
                      }
                      if (!valid42) {
                        const err65 = { instancePath: instancePath + "/sourceLocation", schemaPath: "#/$defs/field_availability/oneOf", keyword: "oneOf", params: { passingSchemas: passing5 }, message: "must match exactly one schema in oneOf" };
                        if (vErrors === null) {
                          vErrors = [err65];
                        } else {
                          vErrors.push(err65);
                        }
                        errors++;
                        validate77.errors = vErrors;
                        return false;
                      } else {
                        errors = _errs120;
                        if (vErrors !== null) {
                          if (_errs120) {
                            vErrors.length = _errs120;
                          } else {
                            vErrors = null;
                          }
                        }
                      }
                      if (errors === _errs118) {
                        if (data65 && typeof data65 == "object" && !Array.isArray(data65)) {
                          let missing6;
                          if (data65.state === void 0 && (missing6 = "state") || data65.reason === void 0 && (missing6 = "reason")) {
                            validate77.errors = [{ instancePath: instancePath + "/sourceLocation", schemaPath: "#/$defs/field_availability/required", keyword: "required", params: { missingProperty: missing6 }, message: "must have required property '" + missing6 + "'" }];
                            return false;
                          } else {
                            const _errs136 = errors;
                            for (const key6 in data65) {
                              if (!(key6 === "state" || key6 === "reason")) {
                                validate77.errors = [{ instancePath: instancePath + "/sourceLocation", schemaPath: "#/$defs/field_availability/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key6 }, message: "must NOT have additional properties" }];
                                return false;
                                break;
                              }
                            }
                            if (_errs136 === errors) {
                              if (data65.state !== void 0) {
                                let data76 = data65.state;
                                const _errs137 = errors;
                                if (!(data76 === "available" || data76 === "source_unavailable" || data76 === "withheld" || data76 === "not_applicable" || data76 === "private_lookup")) {
                                  validate77.errors = [{ instancePath: instancePath + "/sourceLocation/state", schemaPath: "#/$defs/field_availability/properties/state/enum", keyword: "enum", params: { allowedValues: schema107.properties.state.enum }, message: "must be equal to one of the allowed values" }];
                                  return false;
                                }
                                var valid48 = _errs137 === errors;
                              } else {
                                var valid48 = true;
                              }
                              if (valid48) {
                                if (data65.reason !== void 0) {
                                  let data77 = data65.reason;
                                  const _errs138 = errors;
                                  if (typeof data77 !== "string") {
                                    validate77.errors = [{ instancePath: instancePath + "/sourceLocation/reason", schemaPath: "#/$defs/field_availability/properties/reason/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                    return false;
                                  }
                                  if (!(data77 === "reported_by_adapter" || data77 === "derived_from_trace_context" || data77 === "legacy_v1_report" || data77 === "source_not_provided" || data77 === "not_evaluated" || data77 === "partial_token_metrics" || data77 === "historical_codex_source_not_lookup_eligible" || data77 === "codex_notify_turn_correlation_unavailable" || data77 === "ambiguous_trace_repository" || data77 === "span_kind_not_model_backed" || data77 === "span_kind_has_no_latency" || data77 === "span_kind_has_no_token_usage" || data77 === "claude_private_lookup_not_supported" || data77 === "cursor_private_lookup_not_supported" || data77 === "codex_span_not_notify_derived" || data77 === "agent_private_lookup_not_supported" || data77 === "local_opt_in_lookup_required" || data77 === "withheld_by_privacy_policy")) {
                                    validate77.errors = [{ instancePath: instancePath + "/sourceLocation/reason", schemaPath: "#/$defs/field_availability/properties/reason/enum", keyword: "enum", params: { allowedValues: schema107.properties.reason.enum }, message: "must be equal to one of the allowed values" }];
                                    return false;
                                  }
                                  var valid48 = _errs138 === errors;
                                } else {
                                  var valid48 = true;
                                }
                              }
                            }
                          }
                        } else {
                          validate77.errors = [{ instancePath: instancePath + "/sourceLocation", schemaPath: "#/$defs/field_availability/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
                          return false;
                        }
                      }
                      var valid0 = _errs117 === errors;
                    } else {
                      var valid0 = true;
                    }
                    if (valid0) {
                      if (data.requestContent !== void 0) {
                        let data78 = data.requestContent;
                        const _errs140 = errors;
                        const _errs141 = errors;
                        const _errs143 = errors;
                        let valid50 = false;
                        let passing6 = null;
                        const _errs144 = errors;
                        if (data78 && typeof data78 == "object" && !Array.isArray(data78)) {
                          if (data78.state !== void 0) {
                            const _errs145 = errors;
                            if ("available" !== data78.state) {
                              const err66 = { instancePath: instancePath + "/requestContent/state", schemaPath: "#/$defs/field_availability/oneOf/0/properties/state/const", keyword: "const", params: { allowedValue: "available" }, message: "must be equal to constant" };
                              if (vErrors === null) {
                                vErrors = [err66];
                              } else {
                                vErrors.push(err66);
                              }
                              errors++;
                            }
                            var valid51 = _errs145 === errors;
                          } else {
                            var valid51 = true;
                          }
                          if (valid51) {
                            if (data78.reason !== void 0) {
                              let data80 = data78.reason;
                              const _errs146 = errors;
                              if (!(data80 === "reported_by_adapter" || data80 === "derived_from_trace_context" || data80 === "legacy_v1_report")) {
                                const err67 = { instancePath: instancePath + "/requestContent/reason", schemaPath: "#/$defs/field_availability/oneOf/0/properties/reason/enum", keyword: "enum", params: { allowedValues: schema107.oneOf[0].properties.reason.enum }, message: "must be equal to one of the allowed values" };
                                if (vErrors === null) {
                                  vErrors = [err67];
                                } else {
                                  vErrors.push(err67);
                                }
                                errors++;
                              }
                              var valid51 = _errs146 === errors;
                            } else {
                              var valid51 = true;
                            }
                          }
                        }
                        var _valid6 = _errs144 === errors;
                        if (_valid6) {
                          valid50 = true;
                          passing6 = 0;
                          var props6 = {};
                          props6.state = true;
                          props6.reason = true;
                        }
                        const _errs147 = errors;
                        if (data78 && typeof data78 == "object" && !Array.isArray(data78)) {
                          if (data78.state !== void 0) {
                            const _errs148 = errors;
                            if ("source_unavailable" !== data78.state) {
                              const err68 = { instancePath: instancePath + "/requestContent/state", schemaPath: "#/$defs/field_availability/oneOf/1/properties/state/const", keyword: "const", params: { allowedValue: "source_unavailable" }, message: "must be equal to constant" };
                              if (vErrors === null) {
                                vErrors = [err68];
                              } else {
                                vErrors.push(err68);
                              }
                              errors++;
                            }
                            var valid52 = _errs148 === errors;
                          } else {
                            var valid52 = true;
                          }
                          if (valid52) {
                            if (data78.reason !== void 0) {
                              let data82 = data78.reason;
                              const _errs149 = errors;
                              if (!(data82 === "source_not_provided" || data82 === "not_evaluated" || data82 === "partial_token_metrics" || data82 === "historical_codex_source_not_lookup_eligible" || data82 === "codex_notify_turn_correlation_unavailable" || data82 === "ambiguous_trace_repository" || data82 === "legacy_v1_report")) {
                                const err69 = { instancePath: instancePath + "/requestContent/reason", schemaPath: "#/$defs/field_availability/oneOf/1/properties/reason/enum", keyword: "enum", params: { allowedValues: schema107.oneOf[1].properties.reason.enum }, message: "must be equal to one of the allowed values" };
                                if (vErrors === null) {
                                  vErrors = [err69];
                                } else {
                                  vErrors.push(err69);
                                }
                                errors++;
                              }
                              var valid52 = _errs149 === errors;
                            } else {
                              var valid52 = true;
                            }
                          }
                        }
                        var _valid6 = _errs147 === errors;
                        if (_valid6 && valid50) {
                          valid50 = false;
                          passing6 = [passing6, 1];
                        } else {
                          if (_valid6) {
                            valid50 = true;
                            passing6 = 1;
                            if (props6 !== true) {
                              props6 = props6 || {};
                              props6.state = true;
                              props6.reason = true;
                            }
                          }
                          const _errs150 = errors;
                          if (data78 && typeof data78 == "object" && !Array.isArray(data78)) {
                            if (data78.state !== void 0) {
                              const _errs151 = errors;
                              if ("not_applicable" !== data78.state) {
                                const err70 = { instancePath: instancePath + "/requestContent/state", schemaPath: "#/$defs/field_availability/oneOf/2/properties/state/const", keyword: "const", params: { allowedValue: "not_applicable" }, message: "must be equal to constant" };
                                if (vErrors === null) {
                                  vErrors = [err70];
                                } else {
                                  vErrors.push(err70);
                                }
                                errors++;
                              }
                              var valid53 = _errs151 === errors;
                            } else {
                              var valid53 = true;
                            }
                            if (valid53) {
                              if (data78.reason !== void 0) {
                                let data84 = data78.reason;
                                const _errs152 = errors;
                                if (!(data84 === "span_kind_not_model_backed" || data84 === "span_kind_has_no_latency" || data84 === "span_kind_has_no_token_usage" || data84 === "claude_private_lookup_not_supported" || data84 === "cursor_private_lookup_not_supported" || data84 === "codex_span_not_notify_derived" || data84 === "agent_private_lookup_not_supported")) {
                                  const err71 = { instancePath: instancePath + "/requestContent/reason", schemaPath: "#/$defs/field_availability/oneOf/2/properties/reason/enum", keyword: "enum", params: { allowedValues: schema107.oneOf[2].properties.reason.enum }, message: "must be equal to one of the allowed values" };
                                  if (vErrors === null) {
                                    vErrors = [err71];
                                  } else {
                                    vErrors.push(err71);
                                  }
                                  errors++;
                                }
                                var valid53 = _errs152 === errors;
                              } else {
                                var valid53 = true;
                              }
                            }
                          }
                          var _valid6 = _errs150 === errors;
                          if (_valid6 && valid50) {
                            valid50 = false;
                            passing6 = [passing6, 2];
                          } else {
                            if (_valid6) {
                              valid50 = true;
                              passing6 = 2;
                              if (props6 !== true) {
                                props6 = props6 || {};
                                props6.state = true;
                                props6.reason = true;
                              }
                            }
                            const _errs153 = errors;
                            if (data78 && typeof data78 == "object" && !Array.isArray(data78)) {
                              if (data78.state !== void 0) {
                                const _errs154 = errors;
                                if ("private_lookup" !== data78.state) {
                                  const err72 = { instancePath: instancePath + "/requestContent/state", schemaPath: "#/$defs/field_availability/oneOf/3/properties/state/const", keyword: "const", params: { allowedValue: "private_lookup" }, message: "must be equal to constant" };
                                  if (vErrors === null) {
                                    vErrors = [err72];
                                  } else {
                                    vErrors.push(err72);
                                  }
                                  errors++;
                                }
                                var valid54 = _errs154 === errors;
                              } else {
                                var valid54 = true;
                              }
                              if (valid54) {
                                if (data78.reason !== void 0) {
                                  const _errs155 = errors;
                                  if ("local_opt_in_lookup_required" !== data78.reason) {
                                    const err73 = { instancePath: instancePath + "/requestContent/reason", schemaPath: "#/$defs/field_availability/oneOf/3/properties/reason/const", keyword: "const", params: { allowedValue: "local_opt_in_lookup_required" }, message: "must be equal to constant" };
                                    if (vErrors === null) {
                                      vErrors = [err73];
                                    } else {
                                      vErrors.push(err73);
                                    }
                                    errors++;
                                  }
                                  var valid54 = _errs155 === errors;
                                } else {
                                  var valid54 = true;
                                }
                              }
                            }
                            var _valid6 = _errs153 === errors;
                            if (_valid6 && valid50) {
                              valid50 = false;
                              passing6 = [passing6, 3];
                            } else {
                              if (_valid6) {
                                valid50 = true;
                                passing6 = 3;
                                if (props6 !== true) {
                                  props6 = props6 || {};
                                  props6.state = true;
                                  props6.reason = true;
                                }
                              }
                              const _errs156 = errors;
                              if (data78 && typeof data78 == "object" && !Array.isArray(data78)) {
                                if (data78.state !== void 0) {
                                  const _errs157 = errors;
                                  if ("withheld" !== data78.state) {
                                    const err74 = { instancePath: instancePath + "/requestContent/state", schemaPath: "#/$defs/field_availability/oneOf/4/properties/state/const", keyword: "const", params: { allowedValue: "withheld" }, message: "must be equal to constant" };
                                    if (vErrors === null) {
                                      vErrors = [err74];
                                    } else {
                                      vErrors.push(err74);
                                    }
                                    errors++;
                                  }
                                  var valid55 = _errs157 === errors;
                                } else {
                                  var valid55 = true;
                                }
                                if (valid55) {
                                  if (data78.reason !== void 0) {
                                    const _errs158 = errors;
                                    if ("withheld_by_privacy_policy" !== data78.reason) {
                                      const err75 = { instancePath: instancePath + "/requestContent/reason", schemaPath: "#/$defs/field_availability/oneOf/4/properties/reason/const", keyword: "const", params: { allowedValue: "withheld_by_privacy_policy" }, message: "must be equal to constant" };
                                      if (vErrors === null) {
                                        vErrors = [err75];
                                      } else {
                                        vErrors.push(err75);
                                      }
                                      errors++;
                                    }
                                    var valid55 = _errs158 === errors;
                                  } else {
                                    var valid55 = true;
                                  }
                                }
                              }
                              var _valid6 = _errs156 === errors;
                              if (_valid6 && valid50) {
                                valid50 = false;
                                passing6 = [passing6, 4];
                              } else {
                                if (_valid6) {
                                  valid50 = true;
                                  passing6 = 4;
                                  if (props6 !== true) {
                                    props6 = props6 || {};
                                    props6.state = true;
                                    props6.reason = true;
                                  }
                                }
                              }
                            }
                          }
                        }
                        if (!valid50) {
                          const err76 = { instancePath: instancePath + "/requestContent", schemaPath: "#/$defs/field_availability/oneOf", keyword: "oneOf", params: { passingSchemas: passing6 }, message: "must match exactly one schema in oneOf" };
                          if (vErrors === null) {
                            vErrors = [err76];
                          } else {
                            vErrors.push(err76);
                          }
                          errors++;
                          validate77.errors = vErrors;
                          return false;
                        } else {
                          errors = _errs143;
                          if (vErrors !== null) {
                            if (_errs143) {
                              vErrors.length = _errs143;
                            } else {
                              vErrors = null;
                            }
                          }
                        }
                        if (errors === _errs141) {
                          if (data78 && typeof data78 == "object" && !Array.isArray(data78)) {
                            let missing7;
                            if (data78.state === void 0 && (missing7 = "state") || data78.reason === void 0 && (missing7 = "reason")) {
                              validate77.errors = [{ instancePath: instancePath + "/requestContent", schemaPath: "#/$defs/field_availability/required", keyword: "required", params: { missingProperty: missing7 }, message: "must have required property '" + missing7 + "'" }];
                              return false;
                            } else {
                              const _errs159 = errors;
                              for (const key7 in data78) {
                                if (!(key7 === "state" || key7 === "reason")) {
                                  validate77.errors = [{ instancePath: instancePath + "/requestContent", schemaPath: "#/$defs/field_availability/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key7 }, message: "must NOT have additional properties" }];
                                  return false;
                                  break;
                                }
                              }
                              if (_errs159 === errors) {
                                if (data78.state !== void 0) {
                                  let data89 = data78.state;
                                  const _errs160 = errors;
                                  if (!(data89 === "available" || data89 === "source_unavailable" || data89 === "withheld" || data89 === "not_applicable" || data89 === "private_lookup")) {
                                    validate77.errors = [{ instancePath: instancePath + "/requestContent/state", schemaPath: "#/$defs/field_availability/properties/state/enum", keyword: "enum", params: { allowedValues: schema107.properties.state.enum }, message: "must be equal to one of the allowed values" }];
                                    return false;
                                  }
                                  var valid56 = _errs160 === errors;
                                } else {
                                  var valid56 = true;
                                }
                                if (valid56) {
                                  if (data78.reason !== void 0) {
                                    let data90 = data78.reason;
                                    const _errs161 = errors;
                                    if (typeof data90 !== "string") {
                                      validate77.errors = [{ instancePath: instancePath + "/requestContent/reason", schemaPath: "#/$defs/field_availability/properties/reason/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                      return false;
                                    }
                                    if (!(data90 === "reported_by_adapter" || data90 === "derived_from_trace_context" || data90 === "legacy_v1_report" || data90 === "source_not_provided" || data90 === "not_evaluated" || data90 === "partial_token_metrics" || data90 === "historical_codex_source_not_lookup_eligible" || data90 === "codex_notify_turn_correlation_unavailable" || data90 === "ambiguous_trace_repository" || data90 === "span_kind_not_model_backed" || data90 === "span_kind_has_no_latency" || data90 === "span_kind_has_no_token_usage" || data90 === "claude_private_lookup_not_supported" || data90 === "cursor_private_lookup_not_supported" || data90 === "codex_span_not_notify_derived" || data90 === "agent_private_lookup_not_supported" || data90 === "local_opt_in_lookup_required" || data90 === "withheld_by_privacy_policy")) {
                                      validate77.errors = [{ instancePath: instancePath + "/requestContent/reason", schemaPath: "#/$defs/field_availability/properties/reason/enum", keyword: "enum", params: { allowedValues: schema107.properties.reason.enum }, message: "must be equal to one of the allowed values" }];
                                      return false;
                                    }
                                    var valid56 = _errs161 === errors;
                                  } else {
                                    var valid56 = true;
                                  }
                                }
                              }
                            }
                          } else {
                            validate77.errors = [{ instancePath: instancePath + "/requestContent", schemaPath: "#/$defs/field_availability/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
                            return false;
                          }
                        }
                        var valid0 = _errs140 === errors;
                      } else {
                        var valid0 = true;
                      }
                      if (valid0) {
                        if (data.responseContent !== void 0) {
                          let data91 = data.responseContent;
                          const _errs163 = errors;
                          const _errs164 = errors;
                          const _errs166 = errors;
                          let valid58 = false;
                          let passing7 = null;
                          const _errs167 = errors;
                          if (data91 && typeof data91 == "object" && !Array.isArray(data91)) {
                            if (data91.state !== void 0) {
                              const _errs168 = errors;
                              if ("available" !== data91.state) {
                                const err77 = { instancePath: instancePath + "/responseContent/state", schemaPath: "#/$defs/field_availability/oneOf/0/properties/state/const", keyword: "const", params: { allowedValue: "available" }, message: "must be equal to constant" };
                                if (vErrors === null) {
                                  vErrors = [err77];
                                } else {
                                  vErrors.push(err77);
                                }
                                errors++;
                              }
                              var valid59 = _errs168 === errors;
                            } else {
                              var valid59 = true;
                            }
                            if (valid59) {
                              if (data91.reason !== void 0) {
                                let data93 = data91.reason;
                                const _errs169 = errors;
                                if (!(data93 === "reported_by_adapter" || data93 === "derived_from_trace_context" || data93 === "legacy_v1_report")) {
                                  const err78 = { instancePath: instancePath + "/responseContent/reason", schemaPath: "#/$defs/field_availability/oneOf/0/properties/reason/enum", keyword: "enum", params: { allowedValues: schema107.oneOf[0].properties.reason.enum }, message: "must be equal to one of the allowed values" };
                                  if (vErrors === null) {
                                    vErrors = [err78];
                                  } else {
                                    vErrors.push(err78);
                                  }
                                  errors++;
                                }
                                var valid59 = _errs169 === errors;
                              } else {
                                var valid59 = true;
                              }
                            }
                          }
                          var _valid7 = _errs167 === errors;
                          if (_valid7) {
                            valid58 = true;
                            passing7 = 0;
                            var props7 = {};
                            props7.state = true;
                            props7.reason = true;
                          }
                          const _errs170 = errors;
                          if (data91 && typeof data91 == "object" && !Array.isArray(data91)) {
                            if (data91.state !== void 0) {
                              const _errs171 = errors;
                              if ("source_unavailable" !== data91.state) {
                                const err79 = { instancePath: instancePath + "/responseContent/state", schemaPath: "#/$defs/field_availability/oneOf/1/properties/state/const", keyword: "const", params: { allowedValue: "source_unavailable" }, message: "must be equal to constant" };
                                if (vErrors === null) {
                                  vErrors = [err79];
                                } else {
                                  vErrors.push(err79);
                                }
                                errors++;
                              }
                              var valid60 = _errs171 === errors;
                            } else {
                              var valid60 = true;
                            }
                            if (valid60) {
                              if (data91.reason !== void 0) {
                                let data95 = data91.reason;
                                const _errs172 = errors;
                                if (!(data95 === "source_not_provided" || data95 === "not_evaluated" || data95 === "partial_token_metrics" || data95 === "historical_codex_source_not_lookup_eligible" || data95 === "codex_notify_turn_correlation_unavailable" || data95 === "ambiguous_trace_repository" || data95 === "legacy_v1_report")) {
                                  const err80 = { instancePath: instancePath + "/responseContent/reason", schemaPath: "#/$defs/field_availability/oneOf/1/properties/reason/enum", keyword: "enum", params: { allowedValues: schema107.oneOf[1].properties.reason.enum }, message: "must be equal to one of the allowed values" };
                                  if (vErrors === null) {
                                    vErrors = [err80];
                                  } else {
                                    vErrors.push(err80);
                                  }
                                  errors++;
                                }
                                var valid60 = _errs172 === errors;
                              } else {
                                var valid60 = true;
                              }
                            }
                          }
                          var _valid7 = _errs170 === errors;
                          if (_valid7 && valid58) {
                            valid58 = false;
                            passing7 = [passing7, 1];
                          } else {
                            if (_valid7) {
                              valid58 = true;
                              passing7 = 1;
                              if (props7 !== true) {
                                props7 = props7 || {};
                                props7.state = true;
                                props7.reason = true;
                              }
                            }
                            const _errs173 = errors;
                            if (data91 && typeof data91 == "object" && !Array.isArray(data91)) {
                              if (data91.state !== void 0) {
                                const _errs174 = errors;
                                if ("not_applicable" !== data91.state) {
                                  const err81 = { instancePath: instancePath + "/responseContent/state", schemaPath: "#/$defs/field_availability/oneOf/2/properties/state/const", keyword: "const", params: { allowedValue: "not_applicable" }, message: "must be equal to constant" };
                                  if (vErrors === null) {
                                    vErrors = [err81];
                                  } else {
                                    vErrors.push(err81);
                                  }
                                  errors++;
                                }
                                var valid61 = _errs174 === errors;
                              } else {
                                var valid61 = true;
                              }
                              if (valid61) {
                                if (data91.reason !== void 0) {
                                  let data97 = data91.reason;
                                  const _errs175 = errors;
                                  if (!(data97 === "span_kind_not_model_backed" || data97 === "span_kind_has_no_latency" || data97 === "span_kind_has_no_token_usage" || data97 === "claude_private_lookup_not_supported" || data97 === "cursor_private_lookup_not_supported" || data97 === "codex_span_not_notify_derived" || data97 === "agent_private_lookup_not_supported")) {
                                    const err82 = { instancePath: instancePath + "/responseContent/reason", schemaPath: "#/$defs/field_availability/oneOf/2/properties/reason/enum", keyword: "enum", params: { allowedValues: schema107.oneOf[2].properties.reason.enum }, message: "must be equal to one of the allowed values" };
                                    if (vErrors === null) {
                                      vErrors = [err82];
                                    } else {
                                      vErrors.push(err82);
                                    }
                                    errors++;
                                  }
                                  var valid61 = _errs175 === errors;
                                } else {
                                  var valid61 = true;
                                }
                              }
                            }
                            var _valid7 = _errs173 === errors;
                            if (_valid7 && valid58) {
                              valid58 = false;
                              passing7 = [passing7, 2];
                            } else {
                              if (_valid7) {
                                valid58 = true;
                                passing7 = 2;
                                if (props7 !== true) {
                                  props7 = props7 || {};
                                  props7.state = true;
                                  props7.reason = true;
                                }
                              }
                              const _errs176 = errors;
                              if (data91 && typeof data91 == "object" && !Array.isArray(data91)) {
                                if (data91.state !== void 0) {
                                  const _errs177 = errors;
                                  if ("private_lookup" !== data91.state) {
                                    const err83 = { instancePath: instancePath + "/responseContent/state", schemaPath: "#/$defs/field_availability/oneOf/3/properties/state/const", keyword: "const", params: { allowedValue: "private_lookup" }, message: "must be equal to constant" };
                                    if (vErrors === null) {
                                      vErrors = [err83];
                                    } else {
                                      vErrors.push(err83);
                                    }
                                    errors++;
                                  }
                                  var valid62 = _errs177 === errors;
                                } else {
                                  var valid62 = true;
                                }
                                if (valid62) {
                                  if (data91.reason !== void 0) {
                                    const _errs178 = errors;
                                    if ("local_opt_in_lookup_required" !== data91.reason) {
                                      const err84 = { instancePath: instancePath + "/responseContent/reason", schemaPath: "#/$defs/field_availability/oneOf/3/properties/reason/const", keyword: "const", params: { allowedValue: "local_opt_in_lookup_required" }, message: "must be equal to constant" };
                                      if (vErrors === null) {
                                        vErrors = [err84];
                                      } else {
                                        vErrors.push(err84);
                                      }
                                      errors++;
                                    }
                                    var valid62 = _errs178 === errors;
                                  } else {
                                    var valid62 = true;
                                  }
                                }
                              }
                              var _valid7 = _errs176 === errors;
                              if (_valid7 && valid58) {
                                valid58 = false;
                                passing7 = [passing7, 3];
                              } else {
                                if (_valid7) {
                                  valid58 = true;
                                  passing7 = 3;
                                  if (props7 !== true) {
                                    props7 = props7 || {};
                                    props7.state = true;
                                    props7.reason = true;
                                  }
                                }
                                const _errs179 = errors;
                                if (data91 && typeof data91 == "object" && !Array.isArray(data91)) {
                                  if (data91.state !== void 0) {
                                    const _errs180 = errors;
                                    if ("withheld" !== data91.state) {
                                      const err85 = { instancePath: instancePath + "/responseContent/state", schemaPath: "#/$defs/field_availability/oneOf/4/properties/state/const", keyword: "const", params: { allowedValue: "withheld" }, message: "must be equal to constant" };
                                      if (vErrors === null) {
                                        vErrors = [err85];
                                      } else {
                                        vErrors.push(err85);
                                      }
                                      errors++;
                                    }
                                    var valid63 = _errs180 === errors;
                                  } else {
                                    var valid63 = true;
                                  }
                                  if (valid63) {
                                    if (data91.reason !== void 0) {
                                      const _errs181 = errors;
                                      if ("withheld_by_privacy_policy" !== data91.reason) {
                                        const err86 = { instancePath: instancePath + "/responseContent/reason", schemaPath: "#/$defs/field_availability/oneOf/4/properties/reason/const", keyword: "const", params: { allowedValue: "withheld_by_privacy_policy" }, message: "must be equal to constant" };
                                        if (vErrors === null) {
                                          vErrors = [err86];
                                        } else {
                                          vErrors.push(err86);
                                        }
                                        errors++;
                                      }
                                      var valid63 = _errs181 === errors;
                                    } else {
                                      var valid63 = true;
                                    }
                                  }
                                }
                                var _valid7 = _errs179 === errors;
                                if (_valid7 && valid58) {
                                  valid58 = false;
                                  passing7 = [passing7, 4];
                                } else {
                                  if (_valid7) {
                                    valid58 = true;
                                    passing7 = 4;
                                    if (props7 !== true) {
                                      props7 = props7 || {};
                                      props7.state = true;
                                      props7.reason = true;
                                    }
                                  }
                                }
                              }
                            }
                          }
                          if (!valid58) {
                            const err87 = { instancePath: instancePath + "/responseContent", schemaPath: "#/$defs/field_availability/oneOf", keyword: "oneOf", params: { passingSchemas: passing7 }, message: "must match exactly one schema in oneOf" };
                            if (vErrors === null) {
                              vErrors = [err87];
                            } else {
                              vErrors.push(err87);
                            }
                            errors++;
                            validate77.errors = vErrors;
                            return false;
                          } else {
                            errors = _errs166;
                            if (vErrors !== null) {
                              if (_errs166) {
                                vErrors.length = _errs166;
                              } else {
                                vErrors = null;
                              }
                            }
                          }
                          if (errors === _errs164) {
                            if (data91 && typeof data91 == "object" && !Array.isArray(data91)) {
                              let missing8;
                              if (data91.state === void 0 && (missing8 = "state") || data91.reason === void 0 && (missing8 = "reason")) {
                                validate77.errors = [{ instancePath: instancePath + "/responseContent", schemaPath: "#/$defs/field_availability/required", keyword: "required", params: { missingProperty: missing8 }, message: "must have required property '" + missing8 + "'" }];
                                return false;
                              } else {
                                const _errs182 = errors;
                                for (const key8 in data91) {
                                  if (!(key8 === "state" || key8 === "reason")) {
                                    validate77.errors = [{ instancePath: instancePath + "/responseContent", schemaPath: "#/$defs/field_availability/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key8 }, message: "must NOT have additional properties" }];
                                    return false;
                                    break;
                                  }
                                }
                                if (_errs182 === errors) {
                                  if (data91.state !== void 0) {
                                    let data102 = data91.state;
                                    const _errs183 = errors;
                                    if (!(data102 === "available" || data102 === "source_unavailable" || data102 === "withheld" || data102 === "not_applicable" || data102 === "private_lookup")) {
                                      validate77.errors = [{ instancePath: instancePath + "/responseContent/state", schemaPath: "#/$defs/field_availability/properties/state/enum", keyword: "enum", params: { allowedValues: schema107.properties.state.enum }, message: "must be equal to one of the allowed values" }];
                                      return false;
                                    }
                                    var valid64 = _errs183 === errors;
                                  } else {
                                    var valid64 = true;
                                  }
                                  if (valid64) {
                                    if (data91.reason !== void 0) {
                                      let data103 = data91.reason;
                                      const _errs184 = errors;
                                      if (typeof data103 !== "string") {
                                        validate77.errors = [{ instancePath: instancePath + "/responseContent/reason", schemaPath: "#/$defs/field_availability/properties/reason/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                        return false;
                                      }
                                      if (!(data103 === "reported_by_adapter" || data103 === "derived_from_trace_context" || data103 === "legacy_v1_report" || data103 === "source_not_provided" || data103 === "not_evaluated" || data103 === "partial_token_metrics" || data103 === "historical_codex_source_not_lookup_eligible" || data103 === "codex_notify_turn_correlation_unavailable" || data103 === "ambiguous_trace_repository" || data103 === "span_kind_not_model_backed" || data103 === "span_kind_has_no_latency" || data103 === "span_kind_has_no_token_usage" || data103 === "claude_private_lookup_not_supported" || data103 === "cursor_private_lookup_not_supported" || data103 === "codex_span_not_notify_derived" || data103 === "agent_private_lookup_not_supported" || data103 === "local_opt_in_lookup_required" || data103 === "withheld_by_privacy_policy")) {
                                        validate77.errors = [{ instancePath: instancePath + "/responseContent/reason", schemaPath: "#/$defs/field_availability/properties/reason/enum", keyword: "enum", params: { allowedValues: schema107.properties.reason.enum }, message: "must be equal to one of the allowed values" }];
                                        return false;
                                      }
                                      var valid64 = _errs184 === errors;
                                    } else {
                                      var valid64 = true;
                                    }
                                  }
                                }
                              }
                            } else {
                              validate77.errors = [{ instancePath: instancePath + "/responseContent", schemaPath: "#/$defs/field_availability/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
                              return false;
                            }
                          }
                          var valid0 = _errs163 === errors;
                        } else {
                          var valid0 = true;
                        }
                      }
                    }
                  }
                }
              }
            }
          }
        }
      }
    } else {
      validate77.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate77.errors = vErrors;
  return errors === 0;
}
validate77.evaluated = { "props": true, "dynamicProps": false, "dynamicItems": false };
var schema115 = { "type": "object", "additionalProperties": false, "properties": { "source": { "$ref": "#/$defs/scalar" }, "event_type": { "$ref": "#/$defs/scalar" }, "envelope_type": { "$ref": "#/$defs/scalar" }, "session_id": { "$ref": "#/$defs/scalar" }, "turn_id": { "$ref": "#/$defs/scalar" }, "request_id": { "$ref": "#/$defs/scalar" }, "call_id": { "$ref": "#/$defs/scalar" }, "tool_name": { "$ref": "#/$defs/scalar" }, "phase": { "$ref": "#/$defs/scalar" }, "exit_code": { "$ref": "#/$defs/scalar" }, "sandbox": { "$ref": "#/$defs/scalar" }, "approval": { "$ref": "#/$defs/scalar" } } };
var schema116 = { "type": ["string", "number", "boolean"] };
function validate79(data, { instancePath = "", parentData, parentDataProperty, rootData = data, dynamicAnchors = {} } = {}) {
  let vErrors = null;
  let errors = 0;
  const evaluated0 = validate79.evaluated;
  if (evaluated0.dynamicProps) {
    evaluated0.props = void 0;
  }
  if (evaluated0.dynamicItems) {
    evaluated0.items = void 0;
  }
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      const _errs1 = errors;
      for (const key0 in data) {
        if (!func27.call(schema115.properties, key0)) {
          validate79.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
          return false;
          break;
        }
      }
      if (_errs1 === errors) {
        if (data.source !== void 0) {
          let data0 = data.source;
          const _errs2 = errors;
          if (typeof data0 !== "string" && !(typeof data0 == "number" && isFinite(data0)) && typeof data0 !== "boolean") {
            validate79.errors = [{ instancePath: instancePath + "/source", schemaPath: "#/$defs/scalar/type", keyword: "type", params: { type: schema116.type }, message: "must be string,number,boolean" }];
            return false;
          }
          var valid0 = _errs2 === errors;
        } else {
          var valid0 = true;
        }
        if (valid0) {
          if (data.event_type !== void 0) {
            let data1 = data.event_type;
            const _errs5 = errors;
            if (typeof data1 !== "string" && !(typeof data1 == "number" && isFinite(data1)) && typeof data1 !== "boolean") {
              validate79.errors = [{ instancePath: instancePath + "/event_type", schemaPath: "#/$defs/scalar/type", keyword: "type", params: { type: schema116.type }, message: "must be string,number,boolean" }];
              return false;
            }
            var valid0 = _errs5 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.envelope_type !== void 0) {
              let data2 = data.envelope_type;
              const _errs8 = errors;
              if (typeof data2 !== "string" && !(typeof data2 == "number" && isFinite(data2)) && typeof data2 !== "boolean") {
                validate79.errors = [{ instancePath: instancePath + "/envelope_type", schemaPath: "#/$defs/scalar/type", keyword: "type", params: { type: schema116.type }, message: "must be string,number,boolean" }];
                return false;
              }
              var valid0 = _errs8 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.session_id !== void 0) {
                let data3 = data.session_id;
                const _errs11 = errors;
                if (typeof data3 !== "string" && !(typeof data3 == "number" && isFinite(data3)) && typeof data3 !== "boolean") {
                  validate79.errors = [{ instancePath: instancePath + "/session_id", schemaPath: "#/$defs/scalar/type", keyword: "type", params: { type: schema116.type }, message: "must be string,number,boolean" }];
                  return false;
                }
                var valid0 = _errs11 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.turn_id !== void 0) {
                  let data4 = data.turn_id;
                  const _errs14 = errors;
                  if (typeof data4 !== "string" && !(typeof data4 == "number" && isFinite(data4)) && typeof data4 !== "boolean") {
                    validate79.errors = [{ instancePath: instancePath + "/turn_id", schemaPath: "#/$defs/scalar/type", keyword: "type", params: { type: schema116.type }, message: "must be string,number,boolean" }];
                    return false;
                  }
                  var valid0 = _errs14 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.request_id !== void 0) {
                    let data5 = data.request_id;
                    const _errs17 = errors;
                    if (typeof data5 !== "string" && !(typeof data5 == "number" && isFinite(data5)) && typeof data5 !== "boolean") {
                      validate79.errors = [{ instancePath: instancePath + "/request_id", schemaPath: "#/$defs/scalar/type", keyword: "type", params: { type: schema116.type }, message: "must be string,number,boolean" }];
                      return false;
                    }
                    var valid0 = _errs17 === errors;
                  } else {
                    var valid0 = true;
                  }
                  if (valid0) {
                    if (data.call_id !== void 0) {
                      let data6 = data.call_id;
                      const _errs20 = errors;
                      if (typeof data6 !== "string" && !(typeof data6 == "number" && isFinite(data6)) && typeof data6 !== "boolean") {
                        validate79.errors = [{ instancePath: instancePath + "/call_id", schemaPath: "#/$defs/scalar/type", keyword: "type", params: { type: schema116.type }, message: "must be string,number,boolean" }];
                        return false;
                      }
                      var valid0 = _errs20 === errors;
                    } else {
                      var valid0 = true;
                    }
                    if (valid0) {
                      if (data.tool_name !== void 0) {
                        let data7 = data.tool_name;
                        const _errs23 = errors;
                        if (typeof data7 !== "string" && !(typeof data7 == "number" && isFinite(data7)) && typeof data7 !== "boolean") {
                          validate79.errors = [{ instancePath: instancePath + "/tool_name", schemaPath: "#/$defs/scalar/type", keyword: "type", params: { type: schema116.type }, message: "must be string,number,boolean" }];
                          return false;
                        }
                        var valid0 = _errs23 === errors;
                      } else {
                        var valid0 = true;
                      }
                      if (valid0) {
                        if (data.phase !== void 0) {
                          let data8 = data.phase;
                          const _errs26 = errors;
                          if (typeof data8 !== "string" && !(typeof data8 == "number" && isFinite(data8)) && typeof data8 !== "boolean") {
                            validate79.errors = [{ instancePath: instancePath + "/phase", schemaPath: "#/$defs/scalar/type", keyword: "type", params: { type: schema116.type }, message: "must be string,number,boolean" }];
                            return false;
                          }
                          var valid0 = _errs26 === errors;
                        } else {
                          var valid0 = true;
                        }
                        if (valid0) {
                          if (data.exit_code !== void 0) {
                            let data9 = data.exit_code;
                            const _errs29 = errors;
                            if (typeof data9 !== "string" && !(typeof data9 == "number" && isFinite(data9)) && typeof data9 !== "boolean") {
                              validate79.errors = [{ instancePath: instancePath + "/exit_code", schemaPath: "#/$defs/scalar/type", keyword: "type", params: { type: schema116.type }, message: "must be string,number,boolean" }];
                              return false;
                            }
                            var valid0 = _errs29 === errors;
                          } else {
                            var valid0 = true;
                          }
                          if (valid0) {
                            if (data.sandbox !== void 0) {
                              let data10 = data.sandbox;
                              const _errs32 = errors;
                              if (typeof data10 !== "string" && !(typeof data10 == "number" && isFinite(data10)) && typeof data10 !== "boolean") {
                                validate79.errors = [{ instancePath: instancePath + "/sandbox", schemaPath: "#/$defs/scalar/type", keyword: "type", params: { type: schema116.type }, message: "must be string,number,boolean" }];
                                return false;
                              }
                              var valid0 = _errs32 === errors;
                            } else {
                              var valid0 = true;
                            }
                            if (valid0) {
                              if (data.approval !== void 0) {
                                let data11 = data.approval;
                                const _errs35 = errors;
                                if (typeof data11 !== "string" && !(typeof data11 == "number" && isFinite(data11)) && typeof data11 !== "boolean") {
                                  validate79.errors = [{ instancePath: instancePath + "/approval", schemaPath: "#/$defs/scalar/type", keyword: "type", params: { type: schema116.type }, message: "must be string,number,boolean" }];
                                  return false;
                                }
                                var valid0 = _errs35 === errors;
                              } else {
                                var valid0 = true;
                              }
                            }
                          }
                        }
                      }
                    }
                  }
                }
              }
            }
          }
        }
      }
    } else {
      validate79.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate79.errors = vErrors;
  return errors === 0;
}
validate79.evaluated = { "props": true, "dynamicProps": false, "dynamicItems": false };
var schema90 = { "type": "object", "additionalProperties": false, "required": ["status", "rate_table", "cost"], "properties": { "status": { "enum": ["estimated", "incomplete", "unknown"] }, "reason": { "type": "string" }, "estimated_cost": { "type": "number" }, "currency": { "type": "string" }, "model": { "type": "string" }, "rate_table": { "type": "object", "additionalProperties": false, "properties": { "version": { "type": "string" }, "unit": { "type": "string" } } }, "cost": { "$ref": "#/$defs/cost_detail" } } };
var schema91 = { "type": "object", "additionalProperties": false, "required": ["assumption"], "properties": { "assumption": { "type": "string" }, "incomplete_count": { "type": "number" }, "unknown_count": { "type": "number" }, "missing": { "$ref": "#/$defs/strings" }, "semantic_errors": { "$ref": "#/$defs/strings" }, "components": { "type": "object", "additionalProperties": { "$ref": "#/$defs/cost_component" } } } };
var schema92 = { "type": "array", "items": { "type": "string" } };
var schema94 = { "type": "object", "additionalProperties": false, "required": ["tokens", "rate_per_1m", "estimated_cost"], "properties": { "tokens": { "type": "number" }, "rate_per_1m": { "type": "number" }, "estimated_cost": { "type": "number" } } };
function validate69(data, { instancePath = "", parentData, parentDataProperty, rootData = data, dynamicAnchors = {} } = {}) {
  let vErrors = null;
  let errors = 0;
  const evaluated0 = validate69.evaluated;
  if (evaluated0.dynamicProps) {
    evaluated0.props = void 0;
  }
  if (evaluated0.dynamicItems) {
    evaluated0.items = void 0;
  }
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.assumption === void 0 && (missing0 = "assumption")) {
        validate69.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "assumption" || key0 === "incomplete_count" || key0 === "unknown_count" || key0 === "missing" || key0 === "semantic_errors" || key0 === "components")) {
            validate69.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.assumption !== void 0) {
            const _errs2 = errors;
            if (typeof data.assumption !== "string") {
              validate69.errors = [{ instancePath: instancePath + "/assumption", schemaPath: "#/properties/assumption/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
              return false;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.incomplete_count !== void 0) {
              let data1 = data.incomplete_count;
              const _errs4 = errors;
              if (!(typeof data1 == "number" && isFinite(data1))) {
                validate69.errors = [{ instancePath: instancePath + "/incomplete_count", schemaPath: "#/properties/incomplete_count/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                return false;
              }
              var valid0 = _errs4 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.unknown_count !== void 0) {
                let data2 = data.unknown_count;
                const _errs6 = errors;
                if (!(typeof data2 == "number" && isFinite(data2))) {
                  validate69.errors = [{ instancePath: instancePath + "/unknown_count", schemaPath: "#/properties/unknown_count/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                  return false;
                }
                var valid0 = _errs6 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.missing !== void 0) {
                  let data3 = data.missing;
                  const _errs8 = errors;
                  const _errs9 = errors;
                  if (errors === _errs9) {
                    if (Array.isArray(data3)) {
                      var valid2 = true;
                      const len0 = data3.length;
                      for (let i0 = 0; i0 < len0; i0++) {
                        const _errs11 = errors;
                        if (typeof data3[i0] !== "string") {
                          validate69.errors = [{ instancePath: instancePath + "/missing/" + i0, schemaPath: "#/$defs/strings/items/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                          return false;
                        }
                        var valid2 = _errs11 === errors;
                        if (!valid2) {
                          break;
                        }
                      }
                    } else {
                      validate69.errors = [{ instancePath: instancePath + "/missing", schemaPath: "#/$defs/strings/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                      return false;
                    }
                  }
                  var valid0 = _errs8 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.semantic_errors !== void 0) {
                    let data5 = data.semantic_errors;
                    const _errs13 = errors;
                    const _errs14 = errors;
                    if (errors === _errs14) {
                      if (Array.isArray(data5)) {
                        var valid4 = true;
                        const len1 = data5.length;
                        for (let i1 = 0; i1 < len1; i1++) {
                          const _errs16 = errors;
                          if (typeof data5[i1] !== "string") {
                            validate69.errors = [{ instancePath: instancePath + "/semantic_errors/" + i1, schemaPath: "#/$defs/strings/items/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                            return false;
                          }
                          var valid4 = _errs16 === errors;
                          if (!valid4) {
                            break;
                          }
                        }
                      } else {
                        validate69.errors = [{ instancePath: instancePath + "/semantic_errors", schemaPath: "#/$defs/strings/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                        return false;
                      }
                    }
                    var valid0 = _errs13 === errors;
                  } else {
                    var valid0 = true;
                  }
                  if (valid0) {
                    if (data.components !== void 0) {
                      let data7 = data.components;
                      const _errs18 = errors;
                      if (errors === _errs18) {
                        if (data7 && typeof data7 == "object" && !Array.isArray(data7)) {
                          for (const key1 in data7) {
                            let data8 = data7[key1];
                            const _errs21 = errors;
                            const _errs22 = errors;
                            if (errors === _errs22) {
                              if (data8 && typeof data8 == "object" && !Array.isArray(data8)) {
                                let missing1;
                                if (data8.tokens === void 0 && (missing1 = "tokens") || data8.rate_per_1m === void 0 && (missing1 = "rate_per_1m") || data8.estimated_cost === void 0 && (missing1 = "estimated_cost")) {
                                  validate69.errors = [{ instancePath: instancePath + "/components/" + key1.replace(/~/g, "~0").replace(/\//g, "~1"), schemaPath: "#/$defs/cost_component/required", keyword: "required", params: { missingProperty: missing1 }, message: "must have required property '" + missing1 + "'" }];
                                  return false;
                                } else {
                                  const _errs24 = errors;
                                  for (const key2 in data8) {
                                    if (!(key2 === "tokens" || key2 === "rate_per_1m" || key2 === "estimated_cost")) {
                                      validate69.errors = [{ instancePath: instancePath + "/components/" + key1.replace(/~/g, "~0").replace(/\//g, "~1"), schemaPath: "#/$defs/cost_component/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key2 }, message: "must NOT have additional properties" }];
                                      return false;
                                      break;
                                    }
                                  }
                                  if (_errs24 === errors) {
                                    if (data8.tokens !== void 0) {
                                      let data9 = data8.tokens;
                                      const _errs25 = errors;
                                      if (!(typeof data9 == "number" && isFinite(data9))) {
                                        validate69.errors = [{ instancePath: instancePath + "/components/" + key1.replace(/~/g, "~0").replace(/\//g, "~1") + "/tokens", schemaPath: "#/$defs/cost_component/properties/tokens/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                                        return false;
                                      }
                                      var valid7 = _errs25 === errors;
                                    } else {
                                      var valid7 = true;
                                    }
                                    if (valid7) {
                                      if (data8.rate_per_1m !== void 0) {
                                        let data10 = data8.rate_per_1m;
                                        const _errs27 = errors;
                                        if (!(typeof data10 == "number" && isFinite(data10))) {
                                          validate69.errors = [{ instancePath: instancePath + "/components/" + key1.replace(/~/g, "~0").replace(/\//g, "~1") + "/rate_per_1m", schemaPath: "#/$defs/cost_component/properties/rate_per_1m/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                                          return false;
                                        }
                                        var valid7 = _errs27 === errors;
                                      } else {
                                        var valid7 = true;
                                      }
                                      if (valid7) {
                                        if (data8.estimated_cost !== void 0) {
                                          let data11 = data8.estimated_cost;
                                          const _errs29 = errors;
                                          if (!(typeof data11 == "number" && isFinite(data11))) {
                                            validate69.errors = [{ instancePath: instancePath + "/components/" + key1.replace(/~/g, "~0").replace(/\//g, "~1") + "/estimated_cost", schemaPath: "#/$defs/cost_component/properties/estimated_cost/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                                            return false;
                                          }
                                          var valid7 = _errs29 === errors;
                                        } else {
                                          var valid7 = true;
                                        }
                                      }
                                    }
                                  }
                                }
                              } else {
                                validate69.errors = [{ instancePath: instancePath + "/components/" + key1.replace(/~/g, "~0").replace(/\//g, "~1"), schemaPath: "#/$defs/cost_component/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
                                return false;
                              }
                            }
                            var valid5 = _errs21 === errors;
                            if (!valid5) {
                              break;
                            }
                          }
                        } else {
                          validate69.errors = [{ instancePath: instancePath + "/components", schemaPath: "#/properties/components/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
                          return false;
                        }
                      }
                      var valid0 = _errs18 === errors;
                    } else {
                      var valid0 = true;
                    }
                  }
                }
              }
            }
          }
        }
      }
    } else {
      validate69.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate69.errors = vErrors;
  return errors === 0;
}
validate69.evaluated = { "props": true, "dynamicProps": false, "dynamicItems": false };
function validate68(data, { instancePath = "", parentData, parentDataProperty, rootData = data, dynamicAnchors = {} } = {}) {
  let vErrors = null;
  let errors = 0;
  const evaluated0 = validate68.evaluated;
  if (evaluated0.dynamicProps) {
    evaluated0.props = void 0;
  }
  if (evaluated0.dynamicItems) {
    evaluated0.items = void 0;
  }
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.status === void 0 && (missing0 = "status") || data.rate_table === void 0 && (missing0 = "rate_table") || data.cost === void 0 && (missing0 = "cost")) {
        validate68.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "status" || key0 === "reason" || key0 === "estimated_cost" || key0 === "currency" || key0 === "model" || key0 === "rate_table" || key0 === "cost")) {
            validate68.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.status !== void 0) {
            let data0 = data.status;
            const _errs2 = errors;
            if (!(data0 === "estimated" || data0 === "incomplete" || data0 === "unknown")) {
              validate68.errors = [{ instancePath: instancePath + "/status", schemaPath: "#/properties/status/enum", keyword: "enum", params: { allowedValues: schema90.properties.status.enum }, message: "must be equal to one of the allowed values" }];
              return false;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.reason !== void 0) {
              const _errs3 = errors;
              if (typeof data.reason !== "string") {
                validate68.errors = [{ instancePath: instancePath + "/reason", schemaPath: "#/properties/reason/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                return false;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.estimated_cost !== void 0) {
                let data2 = data.estimated_cost;
                const _errs5 = errors;
                if (!(typeof data2 == "number" && isFinite(data2))) {
                  validate68.errors = [{ instancePath: instancePath + "/estimated_cost", schemaPath: "#/properties/estimated_cost/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                  return false;
                }
                var valid0 = _errs5 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.currency !== void 0) {
                  const _errs7 = errors;
                  if (typeof data.currency !== "string") {
                    validate68.errors = [{ instancePath: instancePath + "/currency", schemaPath: "#/properties/currency/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                    return false;
                  }
                  var valid0 = _errs7 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.model !== void 0) {
                    const _errs9 = errors;
                    if (typeof data.model !== "string") {
                      validate68.errors = [{ instancePath: instancePath + "/model", schemaPath: "#/properties/model/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                      return false;
                    }
                    var valid0 = _errs9 === errors;
                  } else {
                    var valid0 = true;
                  }
                  if (valid0) {
                    if (data.rate_table !== void 0) {
                      let data5 = data.rate_table;
                      const _errs11 = errors;
                      if (errors === _errs11) {
                        if (data5 && typeof data5 == "object" && !Array.isArray(data5)) {
                          const _errs13 = errors;
                          for (const key1 in data5) {
                            if (!(key1 === "version" || key1 === "unit")) {
                              validate68.errors = [{ instancePath: instancePath + "/rate_table", schemaPath: "#/properties/rate_table/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key1 }, message: "must NOT have additional properties" }];
                              return false;
                              break;
                            }
                          }
                          if (_errs13 === errors) {
                            if (data5.version !== void 0) {
                              const _errs14 = errors;
                              if (typeof data5.version !== "string") {
                                validate68.errors = [{ instancePath: instancePath + "/rate_table/version", schemaPath: "#/properties/rate_table/properties/version/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                return false;
                              }
                              var valid1 = _errs14 === errors;
                            } else {
                              var valid1 = true;
                            }
                            if (valid1) {
                              if (data5.unit !== void 0) {
                                const _errs16 = errors;
                                if (typeof data5.unit !== "string") {
                                  validate68.errors = [{ instancePath: instancePath + "/rate_table/unit", schemaPath: "#/properties/rate_table/properties/unit/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                  return false;
                                }
                                var valid1 = _errs16 === errors;
                              } else {
                                var valid1 = true;
                              }
                            }
                          }
                        } else {
                          validate68.errors = [{ instancePath: instancePath + "/rate_table", schemaPath: "#/properties/rate_table/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
                          return false;
                        }
                      }
                      var valid0 = _errs11 === errors;
                    } else {
                      var valid0 = true;
                    }
                    if (valid0) {
                      if (data.cost !== void 0) {
                        const _errs18 = errors;
                        if (!validate69(data.cost, { instancePath: instancePath + "/cost", parentData: data, parentDataProperty: "cost", rootData, dynamicAnchors })) {
                          vErrors = vErrors === null ? validate69.errors : vErrors.concat(validate69.errors);
                          errors = vErrors.length;
                        }
                        var valid0 = _errs18 === errors;
                      } else {
                        var valid0 = true;
                      }
                    }
                  }
                }
              }
            }
          }
        }
      }
    } else {
      validate68.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate68.errors = vErrors;
  return errors === 0;
}
validate68.evaluated = { "props": true, "dynamicProps": false, "dynamicItems": false };
function validate104(data, { instancePath = "", parentData, parentDataProperty, rootData = data, dynamicAnchors = {} } = {}) {
  let vErrors = null;
  let errors = 0;
  const evaluated0 = validate104.evaluated;
  if (evaluated0.dynamicProps) {
    evaluated0.props = void 0;
  }
  if (evaluated0.dynamicItems) {
    evaluated0.items = void 0;
  }
  const _errs2 = errors;
  let valid1 = true;
  const _errs3 = errors;
  if (data && typeof data == "object" && !Array.isArray(data)) {
    if (data.metrics !== void 0) {
      let data0 = data.metrics;
      if (!(data0 && typeof data0 == "object" && !Array.isArray(data0))) {
        const err0 = {};
        if (vErrors === null) {
          vErrors = [err0];
        } else {
          vErrors.push(err0);
        }
        errors++;
      }
      const _errs6 = errors;
      let valid3 = false;
      const _errs7 = errors;
      if (errors === _errs7) {
        if (data0 && typeof data0 == "object" && !Array.isArray(data0)) {
          let missing0;
          if (data0.inputTokens === void 0 && (missing0 = "inputTokens")) {
            const err1 = {};
            if (vErrors === null) {
              vErrors = [err1];
            } else {
              vErrors.push(err1);
            }
            errors++;
          }
        } else {
          const err2 = {};
          if (vErrors === null) {
            vErrors = [err2];
          } else {
            vErrors.push(err2);
          }
          errors++;
        }
      }
      var _valid1 = _errs7 === errors;
      valid3 = valid3 || _valid1;
      if (_valid1) {
        var props0 = {};
        props0.inputTokens = true;
      }
      const _errs9 = errors;
      if (errors === _errs9) {
        if (data0 && typeof data0 == "object" && !Array.isArray(data0)) {
          let missing1;
          if (data0.outputTokens === void 0 && (missing1 = "outputTokens")) {
            const err3 = {};
            if (vErrors === null) {
              vErrors = [err3];
            } else {
              vErrors.push(err3);
            }
            errors++;
          }
        } else {
          const err4 = {};
          if (vErrors === null) {
            vErrors = [err4];
          } else {
            vErrors.push(err4);
          }
          errors++;
        }
      }
      var _valid1 = _errs9 === errors;
      valid3 = valid3 || _valid1;
      if (_valid1) {
        if (props0 !== true) {
          props0 = props0 || {};
          props0.outputTokens = true;
        }
      }
      const _errs11 = errors;
      if (errors === _errs11) {
        if (data0 && typeof data0 == "object" && !Array.isArray(data0)) {
          let missing2;
          if (data0.totalTokens === void 0 && (missing2 = "totalTokens")) {
            const err5 = {};
            if (vErrors === null) {
              vErrors = [err5];
            } else {
              vErrors.push(err5);
            }
            errors++;
          }
        } else {
          const err6 = {};
          if (vErrors === null) {
            vErrors = [err6];
          } else {
            vErrors.push(err6);
          }
          errors++;
        }
      }
      var _valid1 = _errs11 === errors;
      valid3 = valid3 || _valid1;
      if (_valid1) {
        if (props0 !== true) {
          props0 = props0 || {};
          props0.totalTokens = true;
        }
      }
      const _errs13 = errors;
      if (errors === _errs13) {
        if (data0 && typeof data0 == "object" && !Array.isArray(data0)) {
          let missing3;
          if (data0.totalInputTokens === void 0 && (missing3 = "totalInputTokens")) {
            const err7 = {};
            if (vErrors === null) {
              vErrors = [err7];
            } else {
              vErrors.push(err7);
            }
            errors++;
          }
        } else {
          const err8 = {};
          if (vErrors === null) {
            vErrors = [err8];
          } else {
            vErrors.push(err8);
          }
          errors++;
        }
      }
      var _valid1 = _errs13 === errors;
      valid3 = valid3 || _valid1;
      if (_valid1) {
        if (props0 !== true) {
          props0 = props0 || {};
          props0.totalInputTokens = true;
        }
      }
      const _errs15 = errors;
      if (errors === _errs15) {
        if (data0 && typeof data0 == "object" && !Array.isArray(data0)) {
          let missing4;
          if (data0.totalOutputTokens === void 0 && (missing4 = "totalOutputTokens")) {
            const err9 = {};
            if (vErrors === null) {
              vErrors = [err9];
            } else {
              vErrors.push(err9);
            }
            errors++;
          }
        } else {
          const err10 = {};
          if (vErrors === null) {
            vErrors = [err10];
          } else {
            vErrors.push(err10);
          }
          errors++;
        }
      }
      var _valid1 = _errs15 === errors;
      valid3 = valid3 || _valid1;
      if (_valid1) {
        if (props0 !== true) {
          props0 = props0 || {};
          props0.totalOutputTokens = true;
        }
      }
      const _errs17 = errors;
      if (errors === _errs17) {
        if (data0 && typeof data0 == "object" && !Array.isArray(data0)) {
          let missing5;
          if (data0.totalAccumulatedTokens === void 0 && (missing5 = "totalAccumulatedTokens")) {
            const err11 = {};
            if (vErrors === null) {
              vErrors = [err11];
            } else {
              vErrors.push(err11);
            }
            errors++;
          }
        } else {
          const err12 = {};
          if (vErrors === null) {
            vErrors = [err12];
          } else {
            vErrors.push(err12);
          }
          errors++;
        }
      }
      var _valid1 = _errs17 === errors;
      valid3 = valid3 || _valid1;
      if (_valid1) {
        if (props0 !== true) {
          props0 = props0 || {};
          props0.totalAccumulatedTokens = true;
        }
      }
      if (!valid3) {
        const err13 = {};
        if (vErrors === null) {
          vErrors = [err13];
        } else {
          vErrors.push(err13);
        }
        errors++;
      } else {
        errors = _errs6;
        if (vErrors !== null) {
          if (_errs6) {
            vErrors.length = _errs6;
          } else {
            vErrors = null;
          }
        }
      }
    }
  }
  var _valid0 = _errs3 === errors;
  errors = _errs2;
  if (vErrors !== null) {
    if (_errs2) {
      vErrors.length = _errs2;
    } else {
      vErrors = null;
    }
  }
  let ifClause0;
  if (_valid0) {
    const _errs19 = errors;
    const _errs20 = errors;
    let valid4 = true;
    const _errs21 = errors;
    if (data && typeof data == "object" && !Array.isArray(data)) {
      if (data.metrics !== void 0) {
        let data1 = data.metrics;
        if (!(data1 && typeof data1 == "object" && !Array.isArray(data1))) {
          const err14 = {};
          if (vErrors === null) {
            vErrors = [err14];
          } else {
            vErrors.push(err14);
          }
          errors++;
        }
        const _errs24 = errors;
        let valid6 = false;
        const _errs25 = errors;
        if (errors === _errs25) {
          if (data1 && typeof data1 == "object" && !Array.isArray(data1)) {
            let missing6;
            if (data1.totalTokens === void 0 && (missing6 = "totalTokens")) {
              const err15 = {};
              if (vErrors === null) {
                vErrors = [err15];
              } else {
                vErrors.push(err15);
              }
              errors++;
            }
          } else {
            const err16 = {};
            if (vErrors === null) {
              vErrors = [err16];
            } else {
              vErrors.push(err16);
            }
            errors++;
          }
        }
        var _valid3 = _errs25 === errors;
        valid6 = valid6 || _valid3;
        if (_valid3) {
          var props1 = {};
          props1.totalTokens = true;
        }
        const _errs27 = errors;
        if (errors === _errs27) {
          if (data1 && typeof data1 == "object" && !Array.isArray(data1)) {
            let missing7;
            if (data1.totalAccumulatedTokens === void 0 && (missing7 = "totalAccumulatedTokens")) {
              const err17 = {};
              if (vErrors === null) {
                vErrors = [err17];
              } else {
                vErrors.push(err17);
              }
              errors++;
            }
          } else {
            const err18 = {};
            if (vErrors === null) {
              vErrors = [err18];
            } else {
              vErrors.push(err18);
            }
            errors++;
          }
        }
        var _valid3 = _errs27 === errors;
        valid6 = valid6 || _valid3;
        if (_valid3) {
          if (props1 !== true) {
            props1 = props1 || {};
            props1.totalAccumulatedTokens = true;
          }
        }
        const _errs29 = errors;
        if (errors === _errs29) {
          if (data1 && typeof data1 == "object" && !Array.isArray(data1)) {
            let missing8;
            if (data1.inputTokens === void 0 && (missing8 = "inputTokens") || data1.outputTokens === void 0 && (missing8 = "outputTokens")) {
              const err19 = {};
              if (vErrors === null) {
                vErrors = [err19];
              } else {
                vErrors.push(err19);
              }
              errors++;
            }
          } else {
            const err20 = {};
            if (vErrors === null) {
              vErrors = [err20];
            } else {
              vErrors.push(err20);
            }
            errors++;
          }
        }
        var _valid3 = _errs29 === errors;
        valid6 = valid6 || _valid3;
        if (_valid3) {
          if (props1 !== true) {
            props1 = props1 || {};
            props1.inputTokens = true;
            props1.outputTokens = true;
          }
        }
        const _errs31 = errors;
        if (errors === _errs31) {
          if (data1 && typeof data1 == "object" && !Array.isArray(data1)) {
            let missing9;
            if (data1.totalInputTokens === void 0 && (missing9 = "totalInputTokens") || data1.totalOutputTokens === void 0 && (missing9 = "totalOutputTokens")) {
              const err21 = {};
              if (vErrors === null) {
                vErrors = [err21];
              } else {
                vErrors.push(err21);
              }
              errors++;
            }
          } else {
            const err22 = {};
            if (vErrors === null) {
              vErrors = [err22];
            } else {
              vErrors.push(err22);
            }
            errors++;
          }
        }
        var _valid3 = _errs31 === errors;
        valid6 = valid6 || _valid3;
        if (_valid3) {
          if (props1 !== true) {
            props1 = props1 || {};
            props1.totalInputTokens = true;
            props1.totalOutputTokens = true;
          }
        }
        if (!valid6) {
          const err23 = {};
          if (vErrors === null) {
            vErrors = [err23];
          } else {
            vErrors.push(err23);
          }
          errors++;
        } else {
          errors = _errs24;
          if (vErrors !== null) {
            if (_errs24) {
              vErrors.length = _errs24;
            } else {
              vErrors = null;
            }
          }
        }
      }
    }
    var _valid2 = _errs21 === errors;
    errors = _errs20;
    if (vErrors !== null) {
      if (_errs20) {
        vErrors.length = _errs20;
      } else {
        vErrors = null;
      }
    }
    let ifClause1;
    if (_valid2) {
      const _errs33 = errors;
      if (data && typeof data == "object" && !Array.isArray(data)) {
        if (data.availability !== void 0) {
          let data2 = data.availability;
          const _errs34 = errors;
          if (errors === _errs34) {
            if (data2 && typeof data2 == "object" && !Array.isArray(data2)) {
              if (data2.tokens !== void 0) {
                let data3 = data2.tokens;
                const _errs36 = errors;
                if (errors === _errs36) {
                  if (data3 && typeof data3 == "object" && !Array.isArray(data3)) {
                    if (data3.state !== void 0) {
                      if ("available" !== data3.state) {
                        validate104.errors = [{ instancePath: instancePath + "/availability/tokens/state", schemaPath: "#/allOf/0/then/then/properties/availability/properties/tokens/properties/state/const", keyword: "const", params: { allowedValue: "available" }, message: "must be equal to constant" }];
                        return false;
                      }
                    }
                  } else {
                    validate104.errors = [{ instancePath: instancePath + "/availability/tokens", schemaPath: "#/allOf/0/then/then/properties/availability/properties/tokens/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
                    return false;
                  }
                }
              }
            } else {
              validate104.errors = [{ instancePath: instancePath + "/availability", schemaPath: "#/allOf/0/then/then/properties/availability/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
              return false;
            }
          }
        }
      }
      var _valid2 = _errs33 === errors;
      valid4 = _valid2;
      if (valid4) {
        var props2 = {};
        props2.availability = true;
        props2.metrics = true;
      }
      ifClause1 = "then";
    } else {
      const _errs39 = errors;
      if (data && typeof data == "object" && !Array.isArray(data)) {
        if (data.availability !== void 0) {
          let data5 = data.availability;
          const _errs40 = errors;
          if (errors === _errs40) {
            if (data5 && typeof data5 == "object" && !Array.isArray(data5)) {
              if (data5.tokens !== void 0) {
                let data6 = data5.tokens;
                const _errs42 = errors;
                if (errors === _errs42) {
                  if (data6 && typeof data6 == "object" && !Array.isArray(data6)) {
                    if (data6.state !== void 0) {
                      const _errs44 = errors;
                      if ("source_unavailable" !== data6.state) {
                        validate104.errors = [{ instancePath: instancePath + "/availability/tokens/state", schemaPath: "#/allOf/0/then/else/properties/availability/properties/tokens/properties/state/const", keyword: "const", params: { allowedValue: "source_unavailable" }, message: "must be equal to constant" }];
                        return false;
                      }
                      var valid12 = _errs44 === errors;
                    } else {
                      var valid12 = true;
                    }
                    if (valid12) {
                      if (data6.reason !== void 0) {
                        const _errs45 = errors;
                        if ("partial_token_metrics" !== data6.reason) {
                          validate104.errors = [{ instancePath: instancePath + "/availability/tokens/reason", schemaPath: "#/allOf/0/then/else/properties/availability/properties/tokens/properties/reason/const", keyword: "const", params: { allowedValue: "partial_token_metrics" }, message: "must be equal to constant" }];
                          return false;
                        }
                        var valid12 = _errs45 === errors;
                      } else {
                        var valid12 = true;
                      }
                    }
                  } else {
                    validate104.errors = [{ instancePath: instancePath + "/availability/tokens", schemaPath: "#/allOf/0/then/else/properties/availability/properties/tokens/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
                    return false;
                  }
                }
              }
            } else {
              validate104.errors = [{ instancePath: instancePath + "/availability", schemaPath: "#/allOf/0/then/else/properties/availability/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
              return false;
            }
          }
        }
      }
      var _valid2 = _errs39 === errors;
      valid4 = _valid2;
      if (valid4) {
        if (props2 !== true) {
          props2 = props2 || {};
          props2.availability = true;
        }
      }
      ifClause1 = "else";
    }
    if (!valid4) {
      const err24 = { instancePath, schemaPath: "#/allOf/0/then/if", keyword: "if", params: { failingKeyword: ifClause1 }, message: 'must match "' + ifClause1 + '" schema' };
      if (vErrors === null) {
        vErrors = [err24];
      } else {
        vErrors.push(err24);
      }
      errors++;
      validate104.errors = vErrors;
      return false;
    }
    var _valid0 = _errs19 === errors;
    valid1 = _valid0;
    if (valid1) {
      if (props2 !== true) {
        props2 = props2 || {};
        props2.metrics = true;
      }
    }
    ifClause0 = "then";
  } else {
    const _errs46 = errors;
    if (data && typeof data == "object" && !Array.isArray(data)) {
      if (data.availability !== void 0) {
        let data9 = data.availability;
        const _errs47 = errors;
        if (errors === _errs47) {
          if (data9 && typeof data9 == "object" && !Array.isArray(data9)) {
            if (data9.tokens !== void 0) {
              let data10 = data9.tokens;
              const _errs49 = errors;
              if (errors === _errs49) {
                if (data10 && typeof data10 == "object" && !Array.isArray(data10)) {
                  if (data10.state !== void 0) {
                    const _errs52 = errors;
                    const _errs53 = errors;
                    if ("available" !== data10.state) {
                      const err25 = {};
                      if (vErrors === null) {
                        vErrors = [err25];
                      } else {
                        vErrors.push(err25);
                      }
                      errors++;
                    }
                    var valid16 = _errs53 === errors;
                    if (valid16) {
                      validate104.errors = [{ instancePath: instancePath + "/availability/tokens/state", schemaPath: "#/allOf/0/else/properties/availability/properties/tokens/properties/state/not", keyword: "not", params: {}, message: "must NOT be valid" }];
                      return false;
                    } else {
                      errors = _errs52;
                      if (vErrors !== null) {
                        if (_errs52) {
                          vErrors.length = _errs52;
                        } else {
                          vErrors = null;
                        }
                      }
                    }
                  }
                } else {
                  validate104.errors = [{ instancePath: instancePath + "/availability/tokens", schemaPath: "#/allOf/0/else/properties/availability/properties/tokens/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
                  return false;
                }
              }
            }
          } else {
            validate104.errors = [{ instancePath: instancePath + "/availability", schemaPath: "#/allOf/0/else/properties/availability/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
            return false;
          }
        }
      }
    }
    var _valid0 = _errs46 === errors;
    valid1 = _valid0;
    if (valid1) {
      if (props2 !== true) {
        props2 = props2 || {};
        props2.availability = true;
      }
    }
    ifClause0 = "else";
  }
  if (!valid1) {
    const err26 = { instancePath, schemaPath: "#/allOf/0/if", keyword: "if", params: { failingKeyword: ifClause0 }, message: 'must match "' + ifClause0 + '" schema' };
    if (vErrors === null) {
      vErrors = [err26];
    } else {
      vErrors.push(err26);
    }
    errors++;
    validate104.errors = vErrors;
    return false;
  }
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing10;
      if (data.schemaVersion === void 0 && (missing10 = "schemaVersion") || data.traceId === void 0 && (missing10 = "traceId") || data.spanId === void 0 && (missing10 = "spanId") || data.parentSpanId === void 0 && (missing10 = "parentSpanId") || data.kind === void 0 && (missing10 = "kind") || data.name === void 0 && (missing10 = "name") || data.status === void 0 && (missing10 = "status") || data.startTimeUnixMs === void 0 && (missing10 = "startTimeUnixMs") || data.endTimeUnixMs === void 0 && (missing10 = "endTimeUnixMs") || data.repo === void 0 && (missing10 = "repo") || data.agent === void 0 && (missing10 = "agent") || data.availability === void 0 && (missing10 = "availability") || data.attributes === void 0 && (missing10 = "attributes") || data.metrics === void 0 && (missing10 = "metrics") || data.cost === void 0 && (missing10 = "cost")) {
        validate104.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing10 }, message: "must have required property '" + missing10 + "'" }];
        return false;
      } else {
        const _errs54 = errors;
        for (const key0 in data) {
          if (!func27.call(schema104.properties, key0)) {
            validate104.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs54 === errors) {
          if (data.schemaVersion !== void 0) {
            const _errs55 = errors;
            if (typeof data.schemaVersion !== "string") {
              validate104.errors = [{ instancePath: instancePath + "/schemaVersion", schemaPath: "#/properties/schemaVersion/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
              return false;
            }
            var valid17 = _errs55 === errors;
          } else {
            var valid17 = true;
          }
          if (valid17) {
            if (data.traceId !== void 0) {
              const _errs57 = errors;
              if (typeof data.traceId !== "string") {
                validate104.errors = [{ instancePath: instancePath + "/traceId", schemaPath: "#/properties/traceId/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                return false;
              }
              var valid17 = _errs57 === errors;
            } else {
              var valid17 = true;
            }
            if (valid17) {
              if (data.spanId !== void 0) {
                const _errs59 = errors;
                if (typeof data.spanId !== "string") {
                  validate104.errors = [{ instancePath: instancePath + "/spanId", schemaPath: "#/properties/spanId/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                  return false;
                }
                var valid17 = _errs59 === errors;
              } else {
                var valid17 = true;
              }
              if (valid17) {
                if (data.parentSpanId !== void 0) {
                  let data15 = data.parentSpanId;
                  const _errs61 = errors;
                  if (typeof data15 !== "string" && data15 !== null) {
                    validate104.errors = [{ instancePath: instancePath + "/parentSpanId", schemaPath: "#/properties/parentSpanId/type", keyword: "type", params: { type: schema104.properties.parentSpanId.type }, message: "must be string,null" }];
                    return false;
                  }
                  var valid17 = _errs61 === errors;
                } else {
                  var valid17 = true;
                }
                if (valid17) {
                  if (data.kind !== void 0) {
                    const _errs63 = errors;
                    if (typeof data.kind !== "string") {
                      validate104.errors = [{ instancePath: instancePath + "/kind", schemaPath: "#/properties/kind/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                      return false;
                    }
                    var valid17 = _errs63 === errors;
                  } else {
                    var valid17 = true;
                  }
                  if (valid17) {
                    if (data.name !== void 0) {
                      const _errs65 = errors;
                      if (typeof data.name !== "string") {
                        validate104.errors = [{ instancePath: instancePath + "/name", schemaPath: "#/properties/name/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                        return false;
                      }
                      var valid17 = _errs65 === errors;
                    } else {
                      var valid17 = true;
                    }
                    if (valid17) {
                      if (data.status !== void 0) {
                        const _errs67 = errors;
                        if (typeof data.status !== "string") {
                          validate104.errors = [{ instancePath: instancePath + "/status", schemaPath: "#/properties/status/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                          return false;
                        }
                        var valid17 = _errs67 === errors;
                      } else {
                        var valid17 = true;
                      }
                      if (valid17) {
                        if (data.startTimeUnixMs !== void 0) {
                          let data19 = data.startTimeUnixMs;
                          const _errs69 = errors;
                          if (!(typeof data19 == "number" && isFinite(data19))) {
                            validate104.errors = [{ instancePath: instancePath + "/startTimeUnixMs", schemaPath: "#/properties/startTimeUnixMs/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                            return false;
                          }
                          var valid17 = _errs69 === errors;
                        } else {
                          var valid17 = true;
                        }
                        if (valid17) {
                          if (data.endTimeUnixMs !== void 0) {
                            let data20 = data.endTimeUnixMs;
                            const _errs71 = errors;
                            if (!(typeof data20 == "number" && isFinite(data20)) && data20 !== null) {
                              validate104.errors = [{ instancePath: instancePath + "/endTimeUnixMs", schemaPath: "#/properties/endTimeUnixMs/type", keyword: "type", params: { type: schema104.properties.endTimeUnixMs.type }, message: "must be number,null" }];
                              return false;
                            }
                            var valid17 = _errs71 === errors;
                          } else {
                            var valid17 = true;
                          }
                          if (valid17) {
                            if (data.repo !== void 0) {
                              const _errs73 = errors;
                              if (typeof data.repo !== "string") {
                                validate104.errors = [{ instancePath: instancePath + "/repo", schemaPath: "#/properties/repo/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                return false;
                              }
                              var valid17 = _errs73 === errors;
                            } else {
                              var valid17 = true;
                            }
                            if (valid17) {
                              if (data.agent !== void 0) {
                                let data22 = data.agent;
                                const _errs75 = errors;
                                const _errs76 = errors;
                                if (errors === _errs76) {
                                  if (data22 && typeof data22 == "object" && !Array.isArray(data22)) {
                                    const _errs78 = errors;
                                    for (const key1 in data22) {
                                      if (!(key1 === "name" || key1 === "model" || key1 === "version")) {
                                        validate104.errors = [{ instancePath: instancePath + "/agent", schemaPath: "#/$defs/agent/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key1 }, message: "must NOT have additional properties" }];
                                        return false;
                                        break;
                                      }
                                    }
                                    if (_errs78 === errors) {
                                      if (data22.name !== void 0) {
                                        const _errs79 = errors;
                                        if (typeof data22.name !== "string") {
                                          validate104.errors = [{ instancePath: instancePath + "/agent/name", schemaPath: "#/$defs/agent/properties/name/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                          return false;
                                        }
                                        var valid19 = _errs79 === errors;
                                      } else {
                                        var valid19 = true;
                                      }
                                      if (valid19) {
                                        if (data22.model !== void 0) {
                                          const _errs81 = errors;
                                          if (typeof data22.model !== "string") {
                                            validate104.errors = [{ instancePath: instancePath + "/agent/model", schemaPath: "#/$defs/agent/properties/model/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                            return false;
                                          }
                                          var valid19 = _errs81 === errors;
                                        } else {
                                          var valid19 = true;
                                        }
                                        if (valid19) {
                                          if (data22.version !== void 0) {
                                            const _errs83 = errors;
                                            if (typeof data22.version !== "string") {
                                              validate104.errors = [{ instancePath: instancePath + "/agent/version", schemaPath: "#/$defs/agent/properties/version/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                              return false;
                                            }
                                            var valid19 = _errs83 === errors;
                                          } else {
                                            var valid19 = true;
                                          }
                                        }
                                      }
                                    }
                                  } else {
                                    validate104.errors = [{ instancePath: instancePath + "/agent", schemaPath: "#/$defs/agent/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
                                    return false;
                                  }
                                }
                                var valid17 = _errs75 === errors;
                              } else {
                                var valid17 = true;
                              }
                              if (valid17) {
                                if (data.availability !== void 0) {
                                  const _errs85 = errors;
                                  if (!validate77(data.availability, { instancePath: instancePath + "/availability", parentData: data, parentDataProperty: "availability", rootData, dynamicAnchors })) {
                                    vErrors = vErrors === null ? validate77.errors : vErrors.concat(validate77.errors);
                                    errors = vErrors.length;
                                  }
                                  var valid17 = _errs85 === errors;
                                } else {
                                  var valid17 = true;
                                }
                                if (valid17) {
                                  if (data.sessionId !== void 0) {
                                    const _errs86 = errors;
                                    if (typeof data.sessionId !== "string") {
                                      validate104.errors = [{ instancePath: instancePath + "/sessionId", schemaPath: "#/properties/sessionId/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                      return false;
                                    }
                                    var valid17 = _errs86 === errors;
                                  } else {
                                    var valid17 = true;
                                  }
                                  if (valid17) {
                                    if (data.turnId !== void 0) {
                                      const _errs88 = errors;
                                      if (typeof data.turnId !== "string") {
                                        validate104.errors = [{ instancePath: instancePath + "/turnId", schemaPath: "#/properties/turnId/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                        return false;
                                      }
                                      var valid17 = _errs88 === errors;
                                    } else {
                                      var valid17 = true;
                                    }
                                    if (valid17) {
                                      if (data.toolName !== void 0) {
                                        const _errs90 = errors;
                                        if (typeof data.toolName !== "string") {
                                          validate104.errors = [{ instancePath: instancePath + "/toolName", schemaPath: "#/properties/toolName/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                          return false;
                                        }
                                        var valid17 = _errs90 === errors;
                                      } else {
                                        var valid17 = true;
                                      }
                                      if (valid17) {
                                        if (data.attributes !== void 0) {
                                          const _errs92 = errors;
                                          if (!validate79(data.attributes, { instancePath: instancePath + "/attributes", parentData: data, parentDataProperty: "attributes", rootData, dynamicAnchors })) {
                                            vErrors = vErrors === null ? validate79.errors : vErrors.concat(validate79.errors);
                                            errors = vErrors.length;
                                          }
                                          var valid17 = _errs92 === errors;
                                        } else {
                                          var valid17 = true;
                                        }
                                        if (valid17) {
                                          if (data.metrics !== void 0) {
                                            let data31 = data.metrics;
                                            const _errs93 = errors;
                                            const _errs94 = errors;
                                            if (errors === _errs94) {
                                              if (data31 && typeof data31 == "object" && !Array.isArray(data31)) {
                                                const _errs96 = errors;
                                                for (const key2 in data31) {
                                                  if (!func27.call(schema128.properties, key2)) {
                                                    validate104.errors = [{ instancePath: instancePath + "/metrics", schemaPath: "#/$defs/metrics/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key2 }, message: "must NOT have additional properties" }];
                                                    return false;
                                                    break;
                                                  }
                                                }
                                                if (_errs96 === errors) {
                                                  if (data31.inputTokens !== void 0) {
                                                    let data32 = data31.inputTokens;
                                                    const _errs97 = errors;
                                                    if (!(typeof data32 == "number" && isFinite(data32))) {
                                                      validate104.errors = [{ instancePath: instancePath + "/metrics/inputTokens", schemaPath: "#/$defs/metrics/properties/inputTokens/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                                                      return false;
                                                    }
                                                    var valid21 = _errs97 === errors;
                                                  } else {
                                                    var valid21 = true;
                                                  }
                                                  if (valid21) {
                                                    if (data31.outputTokens !== void 0) {
                                                      let data33 = data31.outputTokens;
                                                      const _errs99 = errors;
                                                      if (!(typeof data33 == "number" && isFinite(data33))) {
                                                        validate104.errors = [{ instancePath: instancePath + "/metrics/outputTokens", schemaPath: "#/$defs/metrics/properties/outputTokens/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                                                        return false;
                                                      }
                                                      var valid21 = _errs99 === errors;
                                                    } else {
                                                      var valid21 = true;
                                                    }
                                                    if (valid21) {
                                                      if (data31.cachedInputTokens !== void 0) {
                                                        let data34 = data31.cachedInputTokens;
                                                        const _errs101 = errors;
                                                        if (!(typeof data34 == "number" && isFinite(data34))) {
                                                          validate104.errors = [{ instancePath: instancePath + "/metrics/cachedInputTokens", schemaPath: "#/$defs/metrics/properties/cachedInputTokens/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                                                          return false;
                                                        }
                                                        var valid21 = _errs101 === errors;
                                                      } else {
                                                        var valid21 = true;
                                                      }
                                                      if (valid21) {
                                                        if (data31.cacheCreationInputTokens !== void 0) {
                                                          let data35 = data31.cacheCreationInputTokens;
                                                          const _errs103 = errors;
                                                          if (!(typeof data35 == "number" && isFinite(data35))) {
                                                            validate104.errors = [{ instancePath: instancePath + "/metrics/cacheCreationInputTokens", schemaPath: "#/$defs/metrics/properties/cacheCreationInputTokens/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                                                            return false;
                                                          }
                                                          var valid21 = _errs103 === errors;
                                                        } else {
                                                          var valid21 = true;
                                                        }
                                                        if (valid21) {
                                                          if (data31.reasoningOutputTokens !== void 0) {
                                                            let data36 = data31.reasoningOutputTokens;
                                                            const _errs105 = errors;
                                                            if (!(typeof data36 == "number" && isFinite(data36))) {
                                                              validate104.errors = [{ instancePath: instancePath + "/metrics/reasoningOutputTokens", schemaPath: "#/$defs/metrics/properties/reasoningOutputTokens/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                                                              return false;
                                                            }
                                                            var valid21 = _errs105 === errors;
                                                          } else {
                                                            var valid21 = true;
                                                          }
                                                          if (valid21) {
                                                            if (data31.totalTokens !== void 0) {
                                                              let data37 = data31.totalTokens;
                                                              const _errs107 = errors;
                                                              if (!(typeof data37 == "number" && isFinite(data37))) {
                                                                validate104.errors = [{ instancePath: instancePath + "/metrics/totalTokens", schemaPath: "#/$defs/metrics/properties/totalTokens/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                                                                return false;
                                                              }
                                                              var valid21 = _errs107 === errors;
                                                            } else {
                                                              var valid21 = true;
                                                            }
                                                            if (valid21) {
                                                              if (data31.latencyMs !== void 0) {
                                                                let data38 = data31.latencyMs;
                                                                const _errs109 = errors;
                                                                if (!(typeof data38 == "number" && isFinite(data38))) {
                                                                  validate104.errors = [{ instancePath: instancePath + "/metrics/latencyMs", schemaPath: "#/$defs/metrics/properties/latencyMs/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                                                                  return false;
                                                                }
                                                                var valid21 = _errs109 === errors;
                                                              } else {
                                                                var valid21 = true;
                                                              }
                                                              if (valid21) {
                                                                if (data31.durationMs !== void 0) {
                                                                  let data39 = data31.durationMs;
                                                                  const _errs111 = errors;
                                                                  if (!(typeof data39 == "number" && isFinite(data39))) {
                                                                    validate104.errors = [{ instancePath: instancePath + "/metrics/durationMs", schemaPath: "#/$defs/metrics/properties/durationMs/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                                                                    return false;
                                                                  }
                                                                  var valid21 = _errs111 === errors;
                                                                } else {
                                                                  var valid21 = true;
                                                                }
                                                                if (valid21) {
                                                                  if (data31.totalInputTokens !== void 0) {
                                                                    let data40 = data31.totalInputTokens;
                                                                    const _errs113 = errors;
                                                                    if (!(typeof data40 == "number" && isFinite(data40))) {
                                                                      validate104.errors = [{ instancePath: instancePath + "/metrics/totalInputTokens", schemaPath: "#/$defs/metrics/properties/totalInputTokens/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                                                                      return false;
                                                                    }
                                                                    var valid21 = _errs113 === errors;
                                                                  } else {
                                                                    var valid21 = true;
                                                                  }
                                                                  if (valid21) {
                                                                    if (data31.totalOutputTokens !== void 0) {
                                                                      let data41 = data31.totalOutputTokens;
                                                                      const _errs115 = errors;
                                                                      if (!(typeof data41 == "number" && isFinite(data41))) {
                                                                        validate104.errors = [{ instancePath: instancePath + "/metrics/totalOutputTokens", schemaPath: "#/$defs/metrics/properties/totalOutputTokens/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                                                                        return false;
                                                                      }
                                                                      var valid21 = _errs115 === errors;
                                                                    } else {
                                                                      var valid21 = true;
                                                                    }
                                                                    if (valid21) {
                                                                      if (data31.totalCachedInputTokens !== void 0) {
                                                                        let data42 = data31.totalCachedInputTokens;
                                                                        const _errs117 = errors;
                                                                        if (!(typeof data42 == "number" && isFinite(data42))) {
                                                                          validate104.errors = [{ instancePath: instancePath + "/metrics/totalCachedInputTokens", schemaPath: "#/$defs/metrics/properties/totalCachedInputTokens/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                                                                          return false;
                                                                        }
                                                                        var valid21 = _errs117 === errors;
                                                                      } else {
                                                                        var valid21 = true;
                                                                      }
                                                                      if (valid21) {
                                                                        if (data31.totalReasoningOutputTokens !== void 0) {
                                                                          let data43 = data31.totalReasoningOutputTokens;
                                                                          const _errs119 = errors;
                                                                          if (!(typeof data43 == "number" && isFinite(data43))) {
                                                                            validate104.errors = [{ instancePath: instancePath + "/metrics/totalReasoningOutputTokens", schemaPath: "#/$defs/metrics/properties/totalReasoningOutputTokens/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                                                                            return false;
                                                                          }
                                                                          var valid21 = _errs119 === errors;
                                                                        } else {
                                                                          var valid21 = true;
                                                                        }
                                                                        if (valid21) {
                                                                          if (data31.totalAccumulatedTokens !== void 0) {
                                                                            let data44 = data31.totalAccumulatedTokens;
                                                                            const _errs121 = errors;
                                                                            if (!(typeof data44 == "number" && isFinite(data44))) {
                                                                              validate104.errors = [{ instancePath: instancePath + "/metrics/totalAccumulatedTokens", schemaPath: "#/$defs/metrics/properties/totalAccumulatedTokens/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                                                                              return false;
                                                                            }
                                                                            var valid21 = _errs121 === errors;
                                                                          } else {
                                                                            var valid21 = true;
                                                                          }
                                                                          if (valid21) {
                                                                            if (data31.contextWindowTokens !== void 0) {
                                                                              let data45 = data31.contextWindowTokens;
                                                                              const _errs123 = errors;
                                                                              if (!(typeof data45 == "number" && isFinite(data45))) {
                                                                                validate104.errors = [{ instancePath: instancePath + "/metrics/contextWindowTokens", schemaPath: "#/$defs/metrics/properties/contextWindowTokens/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                                                                                return false;
                                                                              }
                                                                              var valid21 = _errs123 === errors;
                                                                            } else {
                                                                              var valid21 = true;
                                                                            }
                                                                          }
                                                                        }
                                                                      }
                                                                    }
                                                                  }
                                                                }
                                                              }
                                                            }
                                                          }
                                                        }
                                                      }
                                                    }
                                                  }
                                                }
                                              } else {
                                                validate104.errors = [{ instancePath: instancePath + "/metrics", schemaPath: "#/$defs/metrics/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
                                                return false;
                                              }
                                            }
                                            var valid17 = _errs93 === errors;
                                          } else {
                                            var valid17 = true;
                                          }
                                          if (valid17) {
                                            if (data.estimatedCost !== void 0) {
                                              let data46 = data.estimatedCost;
                                              const _errs125 = errors;
                                              if (!(typeof data46 == "number" && isFinite(data46))) {
                                                validate104.errors = [{ instancePath: instancePath + "/estimatedCost", schemaPath: "#/properties/estimatedCost/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                                                return false;
                                              }
                                              var valid17 = _errs125 === errors;
                                            } else {
                                              var valid17 = true;
                                            }
                                            if (valid17) {
                                              if (data.cost !== void 0) {
                                                const _errs127 = errors;
                                                if (!validate68(data.cost, { instancePath: instancePath + "/cost", parentData: data, parentDataProperty: "cost", rootData, dynamicAnchors })) {
                                                  vErrors = vErrors === null ? validate68.errors : vErrors.concat(validate68.errors);
                                                  errors = vErrors.length;
                                                }
                                                var valid17 = _errs127 === errors;
                                              } else {
                                                var valid17 = true;
                                              }
                                            }
                                          }
                                        }
                                      }
                                    }
                                  }
                                }
                              }
                            }
                          }
                        }
                      }
                    }
                  }
                }
              }
            }
          }
        }
      }
    } else {
      validate104.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate104.errors = vErrors;
  return errors === 0;
}
validate104.evaluated = { "props": true, "dynamicProps": false, "dynamicItems": false };
function validate102(data, { instancePath = "", parentData, parentDataProperty, rootData = data, dynamicAnchors = {} } = {}) {
  let vErrors = null;
  let errors = 0;
  const evaluated0 = validate102.evaluated;
  if (evaluated0.dynamicProps) {
    evaluated0.props = void 0;
  }
  if (evaluated0.dynamicItems) {
    evaluated0.items = void 0;
  }
  const _errs1 = errors;
  if (!validate46(data, { instancePath, parentData, parentDataProperty, rootData, dynamicAnchors })) {
    vErrors = vErrors === null ? validate46.errors : vErrors.concat(validate46.errors);
    errors = vErrors.length;
  }
  var valid0 = _errs1 === errors;
  if (valid0) {
    const _errs2 = errors;
    if (errors === _errs2) {
      if (data && typeof data == "object" && !Array.isArray(data)) {
        let missing0;
        if (data.kind === void 0 && (missing0 = "kind") || data.detail === void 0 && (missing0 = "detail")) {
          validate102.errors = [{ instancePath, schemaPath: "#/allOf/1/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
          return false;
        } else {
          if (data.kind !== void 0) {
            const _errs4 = errors;
            if ("span" !== data.kind) {
              validate102.errors = [{ instancePath: instancePath + "/kind", schemaPath: "#/allOf/1/properties/kind/const", keyword: "const", params: { allowedValue: "span" }, message: "must be equal to constant" }];
              return false;
            }
            var valid1 = _errs4 === errors;
          } else {
            var valid1 = true;
          }
          if (valid1) {
            if (data.detail !== void 0) {
              let data1 = data.detail;
              const _errs5 = errors;
              const _errs6 = errors;
              if (!validate104(data1, { instancePath: instancePath + "/detail", parentData: data, parentDataProperty: "detail", rootData, dynamicAnchors })) {
                vErrors = vErrors === null ? validate104.errors : vErrors.concat(validate104.errors);
                errors = vErrors.length;
              }
              var valid2 = _errs6 === errors;
              if (valid2) {
                const _errs7 = errors;
                if (errors === _errs7) {
                  if (data1 && typeof data1 == "object" && !Array.isArray(data1)) {
                    if (data1.traceId !== void 0) {
                      let data2 = data1.traceId;
                      const _errs9 = errors;
                      const _errs10 = errors;
                      if (errors === _errs10) {
                        if (typeof data2 === "string") {
                          if (!pattern6.test(data2)) {
                            validate102.errors = [{ instancePath: instancePath + "/detail/traceId", schemaPath: "#/$defs/projected_id/pattern", keyword: "pattern", params: { pattern: "^id:sha256:[0-9a-f]{64}$" }, message: 'must match pattern "^id:sha256:[0-9a-f]{64}$"' }];
                            return false;
                          }
                        } else {
                          validate102.errors = [{ instancePath: instancePath + "/detail/traceId", schemaPath: "#/$defs/projected_id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                          return false;
                        }
                      }
                      var valid3 = _errs9 === errors;
                    } else {
                      var valid3 = true;
                    }
                    if (valid3) {
                      if (data1.spanId !== void 0) {
                        let data3 = data1.spanId;
                        const _errs12 = errors;
                        const _errs13 = errors;
                        if (errors === _errs13) {
                          if (typeof data3 === "string") {
                            if (!pattern6.test(data3)) {
                              validate102.errors = [{ instancePath: instancePath + "/detail/spanId", schemaPath: "#/$defs/projected_id/pattern", keyword: "pattern", params: { pattern: "^id:sha256:[0-9a-f]{64}$" }, message: 'must match pattern "^id:sha256:[0-9a-f]{64}$"' }];
                              return false;
                            }
                          } else {
                            validate102.errors = [{ instancePath: instancePath + "/detail/spanId", schemaPath: "#/$defs/projected_id/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                            return false;
                          }
                        }
                        var valid3 = _errs12 === errors;
                      } else {
                        var valid3 = true;
                      }
                      if (valid3) {
                        if (data1.parentSpanId !== void 0) {
                          let data4 = data1.parentSpanId;
                          const _errs15 = errors;
                          const _errs16 = errors;
                          let valid6 = false;
                          let passing0 = null;
                          const _errs17 = errors;
                          const _errs18 = errors;
                          if (errors === _errs18) {
                            if (typeof data4 === "string") {
                              if (!pattern6.test(data4)) {
                                const err0 = { instancePath: instancePath + "/detail/parentSpanId", schemaPath: "#/$defs/projected_id/pattern", keyword: "pattern", params: { pattern: "^id:sha256:[0-9a-f]{64}$" }, message: 'must match pattern "^id:sha256:[0-9a-f]{64}$"' };
                                if (vErrors === null) {
                                  vErrors = [err0];
                                } else {
                                  vErrors.push(err0);
                                }
                                errors++;
                              }
                            } else {
                              const err1 = { instancePath: instancePath + "/detail/parentSpanId", schemaPath: "#/$defs/projected_id/type", keyword: "type", params: { type: "string" }, message: "must be string" };
                              if (vErrors === null) {
                                vErrors = [err1];
                              } else {
                                vErrors.push(err1);
                              }
                              errors++;
                            }
                          }
                          var _valid0 = _errs17 === errors;
                          if (_valid0) {
                            valid6 = true;
                            passing0 = 0;
                          }
                          const _errs20 = errors;
                          if (data4 !== null) {
                            const err2 = { instancePath: instancePath + "/detail/parentSpanId", schemaPath: "#/allOf/1/properties/detail/allOf/1/properties/parentSpanId/oneOf/1/type", keyword: "type", params: { type: "null" }, message: "must be null" };
                            if (vErrors === null) {
                              vErrors = [err2];
                            } else {
                              vErrors.push(err2);
                            }
                            errors++;
                          }
                          var _valid0 = _errs20 === errors;
                          if (_valid0 && valid6) {
                            valid6 = false;
                            passing0 = [passing0, 1];
                          } else {
                            if (_valid0) {
                              valid6 = true;
                              passing0 = 1;
                            }
                          }
                          if (!valid6) {
                            const err3 = { instancePath: instancePath + "/detail/parentSpanId", schemaPath: "#/allOf/1/properties/detail/allOf/1/properties/parentSpanId/oneOf", keyword: "oneOf", params: { passingSchemas: passing0 }, message: "must match exactly one schema in oneOf" };
                            if (vErrors === null) {
                              vErrors = [err3];
                            } else {
                              vErrors.push(err3);
                            }
                            errors++;
                            validate102.errors = vErrors;
                            return false;
                          } else {
                            errors = _errs16;
                            if (vErrors !== null) {
                              if (_errs16) {
                                vErrors.length = _errs16;
                              } else {
                                vErrors = null;
                              }
                            }
                          }
                          var valid3 = _errs15 === errors;
                        } else {
                          var valid3 = true;
                        }
                      }
                    }
                  } else {
                    validate102.errors = [{ instancePath: instancePath + "/detail", schemaPath: "#/allOf/1/properties/detail/allOf/1/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
                    return false;
                  }
                }
                var valid2 = _errs7 === errors;
              }
              var valid1 = _errs5 === errors;
            } else {
              var valid1 = true;
            }
          }
        }
      } else {
        validate102.errors = [{ instancePath, schemaPath: "#/allOf/1/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
        return false;
      }
    }
    var valid0 = _errs2 === errors;
  }
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      for (const key0 in data) {
        if (key0 !== "kind" && key0 !== "detail" && key0 !== "schemaVersion" && key0 !== "snapshot" && key0 !== "scope" && key0 !== "work") {
          validate102.errors = [{ instancePath, schemaPath: "#/unevaluatedProperties", keyword: "unevaluatedProperties", params: { unevaluatedProperty: key0 }, message: "must NOT have unevaluated properties" }];
          return false;
          break;
        }
      }
    } else {
      validate102.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate102.errors = vErrors;
  return errors === 0;
}
validate102.evaluated = { "props": true, "dynamicProps": false, "dynamicItems": false };
var schema151 = { "title": "DashboardFacetsResponseV1", "type": "object", "allOf": [{ "$ref": "#/$defs/response_base" }, { "type": "object", "required": ["kind", "rows", "pagination"], "properties": { "kind": { "const": "facets" }, "rows": { "type": "array", "maxItems": 500, "items": { "$ref": "#/$defs/facet_row" } }, "pagination": { "$ref": "#/$defs/pagination" } } }], "unevaluatedProperties": false };
var schema152 = { "title": "DashboardFacetRowV1", "type": "object", "additionalProperties": false, "required": ["dimension", "value"], "properties": { "dimension": { "$ref": "#/$defs/facet_dimension" }, "value": { "type": "string", "minLength": 1, "maxLength": 256 } } };
var schema153 = { "title": "DashboardFacetDimensionV1", "type": "string", "enum": ["repo", "session", "agent", "model"] };
function validate112(data, { instancePath = "", parentData, parentDataProperty, rootData = data, dynamicAnchors = {} } = {}) {
  let vErrors = null;
  let errors = 0;
  const evaluated0 = validate112.evaluated;
  if (evaluated0.dynamicProps) {
    evaluated0.props = void 0;
  }
  if (evaluated0.dynamicItems) {
    evaluated0.items = void 0;
  }
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.dimension === void 0 && (missing0 = "dimension") || data.value === void 0 && (missing0 = "value")) {
        validate112.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "dimension" || key0 === "value")) {
            validate112.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.dimension !== void 0) {
            let data0 = data.dimension;
            const _errs2 = errors;
            if (typeof data0 !== "string") {
              validate112.errors = [{ instancePath: instancePath + "/dimension", schemaPath: "#/$defs/facet_dimension/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
              return false;
            }
            if (!(data0 === "repo" || data0 === "session" || data0 === "agent" || data0 === "model")) {
              validate112.errors = [{ instancePath: instancePath + "/dimension", schemaPath: "#/$defs/facet_dimension/enum", keyword: "enum", params: { allowedValues: schema153.enum }, message: "must be equal to one of the allowed values" }];
              return false;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.value !== void 0) {
              let data1 = data.value;
              const _errs5 = errors;
              if (errors === _errs5) {
                if (typeof data1 === "string") {
                  if (func1(data1) > 256) {
                    validate112.errors = [{ instancePath: instancePath + "/value", schemaPath: "#/properties/value/maxLength", keyword: "maxLength", params: { limit: 256 }, message: "must NOT have more than 256 characters" }];
                    return false;
                  } else {
                    if (func1(data1) < 1) {
                      validate112.errors = [{ instancePath: instancePath + "/value", schemaPath: "#/properties/value/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                      return false;
                    }
                  }
                } else {
                  validate112.errors = [{ instancePath: instancePath + "/value", schemaPath: "#/properties/value/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                  return false;
                }
              }
              var valid0 = _errs5 === errors;
            } else {
              var valid0 = true;
            }
          }
        }
      }
    } else {
      validate112.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate112.errors = vErrors;
  return errors === 0;
}
validate112.evaluated = { "props": true, "dynamicProps": false, "dynamicItems": false };
function validate110(data, { instancePath = "", parentData, parentDataProperty, rootData = data, dynamicAnchors = {} } = {}) {
  let vErrors = null;
  let errors = 0;
  const evaluated0 = validate110.evaluated;
  if (evaluated0.dynamicProps) {
    evaluated0.props = void 0;
  }
  if (evaluated0.dynamicItems) {
    evaluated0.items = void 0;
  }
  const _errs1 = errors;
  if (!validate46(data, { instancePath, parentData, parentDataProperty, rootData, dynamicAnchors })) {
    vErrors = vErrors === null ? validate46.errors : vErrors.concat(validate46.errors);
    errors = vErrors.length;
  }
  var valid0 = _errs1 === errors;
  if (valid0) {
    const _errs2 = errors;
    if (errors === _errs2) {
      if (data && typeof data == "object" && !Array.isArray(data)) {
        let missing0;
        if (data.kind === void 0 && (missing0 = "kind") || data.rows === void 0 && (missing0 = "rows") || data.pagination === void 0 && (missing0 = "pagination")) {
          validate110.errors = [{ instancePath, schemaPath: "#/allOf/1/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
          return false;
        } else {
          if (data.kind !== void 0) {
            const _errs4 = errors;
            if ("facets" !== data.kind) {
              validate110.errors = [{ instancePath: instancePath + "/kind", schemaPath: "#/allOf/1/properties/kind/const", keyword: "const", params: { allowedValue: "facets" }, message: "must be equal to constant" }];
              return false;
            }
            var valid1 = _errs4 === errors;
          } else {
            var valid1 = true;
          }
          if (valid1) {
            if (data.rows !== void 0) {
              let data1 = data.rows;
              const _errs5 = errors;
              if (errors === _errs5) {
                if (Array.isArray(data1)) {
                  if (data1.length > 500) {
                    validate110.errors = [{ instancePath: instancePath + "/rows", schemaPath: "#/allOf/1/properties/rows/maxItems", keyword: "maxItems", params: { limit: 500 }, message: "must NOT have more than 500 items" }];
                    return false;
                  } else {
                    var valid2 = true;
                    const len0 = data1.length;
                    for (let i0 = 0; i0 < len0; i0++) {
                      const _errs7 = errors;
                      if (!validate112(data1[i0], { instancePath: instancePath + "/rows/" + i0, parentData: data1, parentDataProperty: i0, rootData, dynamicAnchors })) {
                        vErrors = vErrors === null ? validate112.errors : vErrors.concat(validate112.errors);
                        errors = vErrors.length;
                      }
                      var valid2 = _errs7 === errors;
                      if (!valid2) {
                        break;
                      }
                    }
                  }
                } else {
                  validate110.errors = [{ instancePath: instancePath + "/rows", schemaPath: "#/allOf/1/properties/rows/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                  return false;
                }
              }
              var valid1 = _errs5 === errors;
            } else {
              var valid1 = true;
            }
            if (valid1) {
              if (data.pagination !== void 0) {
                const _errs8 = errors;
                if (!validate86(data.pagination, { instancePath: instancePath + "/pagination", parentData: data, parentDataProperty: "pagination", rootData, dynamicAnchors })) {
                  vErrors = vErrors === null ? validate86.errors : vErrors.concat(validate86.errors);
                  errors = vErrors.length;
                }
                var valid1 = _errs8 === errors;
              } else {
                var valid1 = true;
              }
            }
          }
        }
      } else {
        validate110.errors = [{ instancePath, schemaPath: "#/allOf/1/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
        return false;
      }
    }
    var valid0 = _errs2 === errors;
  }
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      for (const key0 in data) {
        if (key0 !== "kind" && key0 !== "rows" && key0 !== "pagination" && key0 !== "schemaVersion" && key0 !== "snapshot" && key0 !== "scope" && key0 !== "work") {
          validate110.errors = [{ instancePath, schemaPath: "#/unevaluatedProperties", keyword: "unevaluatedProperties", params: { unevaluatedProperty: key0 }, message: "must NOT have unevaluated properties" }];
          return false;
          break;
        }
      }
    } else {
      validate110.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate110.errors = vErrors;
  return errors === 0;
}
validate110.evaluated = { "props": true, "dynamicProps": false, "dynamicItems": false };
var schema154 = { "title": "DashboardStatusResponseV1", "type": "object", "additionalProperties": false, "required": ["schemaVersion", "kind", "requestKind", "reason"], "properties": { "schemaVersion": { "const": "agent_observability.dashboard_query.v1" }, "kind": { "const": "status" }, "requestKind": { "$ref": "#/$defs/request_kind" }, "reason": { "$ref": "#/$defs/status_reason" }, "snapshot": { "$ref": "#/$defs/snapshot" } } };
var schema155 = { "title": "DashboardRequestKindV1", "type": "string", "enum": ["bootstrap", "traces", "spans", "summary", "span", "facets"] };
var schema156 = { "title": "DashboardStatusReasonV1", "type": "string", "enum": ["building", "refresh_pending", "snapshot_expired", "busy", "capacity", "invalid_query"] };
function validate116(data, { instancePath = "", parentData, parentDataProperty, rootData = data, dynamicAnchors = {} } = {}) {
  let vErrors = null;
  let errors = 0;
  const evaluated0 = validate116.evaluated;
  if (evaluated0.dynamicProps) {
    evaluated0.props = void 0;
  }
  if (evaluated0.dynamicItems) {
    evaluated0.items = void 0;
  }
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.schemaVersion === void 0 && (missing0 = "schemaVersion") || data.kind === void 0 && (missing0 = "kind") || data.requestKind === void 0 && (missing0 = "requestKind") || data.reason === void 0 && (missing0 = "reason")) {
        validate116.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "schemaVersion" || key0 === "kind" || key0 === "requestKind" || key0 === "reason" || key0 === "snapshot")) {
            validate116.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.schemaVersion !== void 0) {
            const _errs2 = errors;
            if ("agent_observability.dashboard_query.v1" !== data.schemaVersion) {
              validate116.errors = [{ instancePath: instancePath + "/schemaVersion", schemaPath: "#/properties/schemaVersion/const", keyword: "const", params: { allowedValue: "agent_observability.dashboard_query.v1" }, message: "must be equal to constant" }];
              return false;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.kind !== void 0) {
              const _errs3 = errors;
              if ("status" !== data.kind) {
                validate116.errors = [{ instancePath: instancePath + "/kind", schemaPath: "#/properties/kind/const", keyword: "const", params: { allowedValue: "status" }, message: "must be equal to constant" }];
                return false;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.requestKind !== void 0) {
                let data2 = data.requestKind;
                const _errs4 = errors;
                if (typeof data2 !== "string") {
                  validate116.errors = [{ instancePath: instancePath + "/requestKind", schemaPath: "#/$defs/request_kind/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                  return false;
                }
                if (!(data2 === "bootstrap" || data2 === "traces" || data2 === "spans" || data2 === "summary" || data2 === "span" || data2 === "facets")) {
                  validate116.errors = [{ instancePath: instancePath + "/requestKind", schemaPath: "#/$defs/request_kind/enum", keyword: "enum", params: { allowedValues: schema155.enum }, message: "must be equal to one of the allowed values" }];
                  return false;
                }
                var valid0 = _errs4 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.reason !== void 0) {
                  let data3 = data.reason;
                  const _errs7 = errors;
                  if (typeof data3 !== "string") {
                    validate116.errors = [{ instancePath: instancePath + "/reason", schemaPath: "#/$defs/status_reason/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                    return false;
                  }
                  if (!(data3 === "building" || data3 === "refresh_pending" || data3 === "snapshot_expired" || data3 === "busy" || data3 === "capacity" || data3 === "invalid_query")) {
                    validate116.errors = [{ instancePath: instancePath + "/reason", schemaPath: "#/$defs/status_reason/enum", keyword: "enum", params: { allowedValues: schema156.enum }, message: "must be equal to one of the allowed values" }];
                    return false;
                  }
                  var valid0 = _errs7 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.snapshot !== void 0) {
                    const _errs10 = errors;
                    if (!validate47(data.snapshot, { instancePath: instancePath + "/snapshot", parentData: data, parentDataProperty: "snapshot", rootData, dynamicAnchors })) {
                      vErrors = vErrors === null ? validate47.errors : vErrors.concat(validate47.errors);
                      errors = vErrors.length;
                    }
                    var valid0 = _errs10 === errors;
                  } else {
                    var valid0 = true;
                  }
                }
              }
            }
          }
        }
      }
    } else {
      validate116.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate116.errors = vErrors;
  return errors === 0;
}
validate116.evaluated = { "props": true, "dynamicProps": false, "dynamicItems": false };
function validate44(data, { instancePath = "", parentData, parentDataProperty, rootData = data, dynamicAnchors = {} } = {}) {
  let vErrors = null;
  let errors = 0;
  const evaluated0 = validate44.evaluated;
  if (evaluated0.dynamicProps) {
    evaluated0.props = void 0;
  }
  if (evaluated0.dynamicItems) {
    evaluated0.items = void 0;
  }
  const _errs0 = errors;
  let valid0 = false;
  let passing0 = null;
  const _errs1 = errors;
  if (!validate45(data, { instancePath, parentData, parentDataProperty, rootData, dynamicAnchors })) {
    vErrors = vErrors === null ? validate45.errors : vErrors.concat(validate45.errors);
    errors = vErrors.length;
  }
  var _valid0 = _errs1 === errors;
  if (_valid0) {
    valid0 = true;
    passing0 = 0;
    var props0 = true;
  }
  const _errs2 = errors;
  if (!validate62(data, { instancePath, parentData, parentDataProperty, rootData, dynamicAnchors })) {
    vErrors = vErrors === null ? validate62.errors : vErrors.concat(validate62.errors);
    errors = vErrors.length;
  }
  var _valid0 = _errs2 === errors;
  if (_valid0 && valid0) {
    valid0 = false;
    passing0 = [passing0, 1];
  } else {
    if (_valid0) {
      valid0 = true;
      passing0 = 1;
      if (props0 !== true) {
        props0 = true;
      }
    }
    const _errs3 = errors;
    if (!validate91(data, { instancePath, parentData, parentDataProperty, rootData, dynamicAnchors })) {
      vErrors = vErrors === null ? validate91.errors : vErrors.concat(validate91.errors);
      errors = vErrors.length;
    }
    var _valid0 = _errs3 === errors;
    if (_valid0 && valid0) {
      valid0 = false;
      passing0 = [passing0, 2];
    } else {
      if (_valid0) {
        valid0 = true;
        passing0 = 2;
        if (props0 !== true) {
          props0 = true;
        }
      }
      const _errs4 = errors;
      if (!validate98(data, { instancePath, parentData, parentDataProperty, rootData, dynamicAnchors })) {
        vErrors = vErrors === null ? validate98.errors : vErrors.concat(validate98.errors);
        errors = vErrors.length;
      }
      var _valid0 = _errs4 === errors;
      if (_valid0 && valid0) {
        valid0 = false;
        passing0 = [passing0, 3];
      } else {
        if (_valid0) {
          valid0 = true;
          passing0 = 3;
          if (props0 !== true) {
            props0 = true;
          }
        }
        const _errs5 = errors;
        if (!validate102(data, { instancePath, parentData, parentDataProperty, rootData, dynamicAnchors })) {
          vErrors = vErrors === null ? validate102.errors : vErrors.concat(validate102.errors);
          errors = vErrors.length;
        }
        var _valid0 = _errs5 === errors;
        if (_valid0 && valid0) {
          valid0 = false;
          passing0 = [passing0, 4];
        } else {
          if (_valid0) {
            valid0 = true;
            passing0 = 4;
            if (props0 !== true) {
              props0 = true;
            }
          }
          const _errs6 = errors;
          if (!validate110(data, { instancePath, parentData, parentDataProperty, rootData, dynamicAnchors })) {
            vErrors = vErrors === null ? validate110.errors : vErrors.concat(validate110.errors);
            errors = vErrors.length;
          }
          var _valid0 = _errs6 === errors;
          if (_valid0 && valid0) {
            valid0 = false;
            passing0 = [passing0, 5];
          } else {
            if (_valid0) {
              valid0 = true;
              passing0 = 5;
              if (props0 !== true) {
                props0 = true;
              }
            }
            const _errs7 = errors;
            if (!validate116(data, { instancePath, parentData, parentDataProperty, rootData, dynamicAnchors })) {
              vErrors = vErrors === null ? validate116.errors : vErrors.concat(validate116.errors);
              errors = vErrors.length;
            }
            var _valid0 = _errs7 === errors;
            if (_valid0 && valid0) {
              valid0 = false;
              passing0 = [passing0, 6];
            } else {
              if (_valid0) {
                valid0 = true;
                passing0 = 6;
                if (props0 !== true) {
                  props0 = true;
                }
              }
            }
          }
        }
      }
    }
  }
  if (!valid0) {
    const err0 = { instancePath, schemaPath: "#/oneOf", keyword: "oneOf", params: { passingSchemas: passing0 }, message: "must match exactly one schema in oneOf" };
    if (vErrors === null) {
      vErrors = [err0];
    } else {
      vErrors.push(err0);
    }
    errors++;
    validate44.errors = vErrors;
    return false;
  } else {
    errors = _errs0;
    if (vErrors !== null) {
      if (_errs0) {
        vErrors.length = _errs0;
      } else {
        vErrors = null;
      }
    }
  }
  validate44.errors = vErrors;
  evaluated0.props = props0;
  return errors === 0;
}
validate44.evaluated = { "dynamicProps": true, "dynamicItems": false };
function validate20(data, { instancePath = "", parentData, parentDataProperty, rootData = data, dynamicAnchors = {} } = {}) {
  ;
  let vErrors = null;
  let errors = 0;
  const evaluated0 = validate20.evaluated;
  if (evaluated0.dynamicProps) {
    evaluated0.props = void 0;
  }
  if (evaluated0.dynamicItems) {
    evaluated0.items = void 0;
  }
  const _errs1 = errors;
  let valid0 = false;
  let passing0 = null;
  const _errs2 = errors;
  if (!validate21(data, { instancePath, parentData, parentDataProperty, rootData, dynamicAnchors })) {
    vErrors = vErrors === null ? validate21.errors : vErrors.concat(validate21.errors);
    errors = vErrors.length;
  } else {
    var props0 = validate21.evaluated.props;
  }
  var _valid0 = _errs2 === errors;
  if (_valid0) {
    valid0 = true;
    passing0 = 0;
  }
  const _errs3 = errors;
  if (!validate44(data, { instancePath, parentData, parentDataProperty, rootData, dynamicAnchors })) {
    vErrors = vErrors === null ? validate44.errors : vErrors.concat(validate44.errors);
    errors = vErrors.length;
  } else {
    var props1 = validate44.evaluated.props;
  }
  var _valid0 = _errs3 === errors;
  if (_valid0 && valid0) {
    valid0 = false;
    passing0 = [passing0, 1];
  } else {
    if (_valid0) {
      valid0 = true;
      passing0 = 1;
      if (props0 !== true && props1 !== void 0) {
        if (props1 === true) {
          props0 = true;
        } else {
          props0 = props0 || {};
          Object.assign(props0, props1);
        }
      }
    }
  }
  if (!valid0) {
    const err0 = { instancePath, schemaPath: "#/oneOf", keyword: "oneOf", params: { passingSchemas: passing0 }, message: "must match exactly one schema in oneOf" };
    if (vErrors === null) {
      vErrors = [err0];
    } else {
      vErrors.push(err0);
    }
    errors++;
    validate20.errors = vErrors;
    return false;
  } else {
    errors = _errs1;
    if (vErrors !== null) {
      if (_errs1) {
        vErrors.length = _errs1;
      } else {
        vErrors = null;
      }
    }
  }
  validate20.errors = vErrors;
  evaluated0.props = props0;
  return errors === 0;
}
validate20.evaluated = { "dynamicProps": true, "dynamicItems": false };
function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
function isResponseEnvelope(value: unknown): boolean {
  return isRecord(value) && (value.kind === "status" || Object.hasOwn(value, "snapshot"));
}
function withinUtf8Limit(value: unknown, limit: number): boolean {
  try {
    const serialized = JSON.stringify(value);
    return typeof serialized === "string" && new TextEncoder().encode(serialized).byteLength <= limit;
  } catch {
    return false;
  }
}
function isSemanticU64(value: unknown): boolean {
  if (typeof value !== "string" || !/^(0|[1-9][0-9]{0,19})$/.test(value)) return false;
  try {
    return BigInt(value) <= 18446744073709551615n;
  } catch {
    return false;
  }
}
function hasValidSnapshotCounters(value: unknown): boolean {
  if (!isRecord(value)) return false;
  if (value.kind === "status" && value.snapshot === undefined) return true;
  if (!isRecord(value.snapshot)) return false;
  return isSemanticU64(value.snapshot.generation) && isSemanticU64(value.snapshot.visibilityEpoch);
}
export function validateDashboardQueryRequestV1(value: unknown): value is import("./dashboard-query-v1.js").DashboardQueryRequestV1 {
  return validateDashboardWireShape(value) && !isResponseEnvelope(value) && withinUtf8Limit(value, 8192);
}
export default function validateDashboardQueryResponseV1(value: unknown): value is import("./dashboard-query-v1.js").DashboardQueryResponseV1 {
  return validateDashboardWireShape(value) && isResponseEnvelope(value) && hasValidSnapshotCounters(value) && withinUtf8Limit(value, 1048576);
}
