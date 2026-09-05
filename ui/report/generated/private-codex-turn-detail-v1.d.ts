/* Generated from contracts/private-codex-turn-detail-v1.schema.json. Local-only; do not promote or edit. */

/**
 * Local-only private Codex turn detail. This contract must not be promoted to canonical reports, durable records, diagnostics, exports, or team transport.
 */
export interface PrivateCodexTurnDetailV1 {
  schemaVersion: "agent_observability.private_turn_detail.v1";
  turnId: string;
  cwd: string;
  inputMessages: string[];
  lastAssistantMessage: string | null;
}
