import { isCommandEnvelope, isEventEnvelope, parseEventEnvelope } from "../../src/shared/contracts.ts";

const assert = (condition: boolean, message: string) => { if (!condition) throw new Error(message); };
assert(isCommandEnvelope({ version: 1, request_id: "r1", command: { type: "pause" } }), "valid command rejected");
assert(isCommandEnvelope({ version: 1, request_id: "r2", command: { type: "seek", payload: { position_ms: 0 } } }), "zero seek rejected");
assert(!isCommandEnvelope({ version: 1, request_id: "r3", command: { type: "seek", payload: { position_ms: -1 } } }), "negative seek accepted");
assert(!isCommandEnvelope({ version: 1, request_id: "r4", command: { type: "seek", payload: { position_ms: 1.5 } } }), "fractional seek accepted");
assert(!isCommandEnvelope({ version: 1, request_id: "  \t", command: { type: "pause" } }), "blank request id accepted");
assert(!isEventEnvelope({ version: 2, event_id: "e1", generation: 0, job_id: null, session_id: null, event: { type: "error" } }), "unknown version accepted");
assert(isEventEnvelope({ version: 1, event_id: "event-1", generation: 2, job_id: "library-scan", session_id: null, event: { type: "library_root_status", payload: { root_index: 0, status: "missing" } } }), "numeric event ID rejected");
assert(!isEventEnvelope({ version: 1, event_id: "root-1", generation: 2, job_id: "library-scan", session_id: null, event: { type: "library_root_status", payload: { root_index: 0, status: "missing" } } }), "non-numeric event ID accepted");
assert(!isEventEnvelope({ version: 1, event_id: "event-0", generation: 2, job_id: "library-scan", session_id: null, event: { type: "library_root_status", payload: { root_index: 0, status: "missing" } } }), "zero event ID accepted");
assert(!isEventEnvelope({ version: 1, event_id: "event-1.5", generation: 2, job_id: "library-scan", session_id: null, event: { type: "library_root_status", payload: { root_index: 0, status: "missing" } } }), "fractional event ID accepted");
assert(!isEventEnvelope({ version: 1, event_id: "root-2", generation: 2, job_id: null, session_id: null, event: { type: "library_root_status", payload: { root_index: Number.MAX_SAFE_INTEGER + 1, status: "missing" } } }), "unsafe root index accepted");
assert(isEventEnvelope({ version: 1, event_id: "event-2", generation: 0, job_id: null, session_id: null, event: { type: "shutdown", payload: { envelope: { version: 1, state: "timed_out", reason: "system" } } } }), "timed out shutdown rejected");
assert(!isEventEnvelope({ version: 1, event_id: "error-1", generation: 2, job_id: null, session_id: null, event: { type: "error", payload: { error: { code: "internal", message: "x" } } } }), "incomplete error accepted");
const validError = { version: 1, event_id: "delivery-error-1", generation: 0, job_id: null, session_id: null, event: { type: "error", payload: { error: { code: "internal", message: "x", retryable: true } } } };
assert(isEventEnvelope(validError), "delivery error ID rejected");
for (const id of ["event-01", "event-9007199254740992", "delivery-error-9007199254740992", "delivery-error-1.5"]) assert(!isEventEnvelope({ ...validError, event_id: id }), `malformed ID accepted: ${id}`);
let rejected = false;
try { parseEventEnvelope(JSON.stringify({ version: 1, event_id: "e1", generation: 0, job_id: null, session_id: null, event: {}, extra: true })); } catch { rejected = true; }
assert(rejected, "malformed event accepted");
