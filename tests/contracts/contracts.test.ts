import { isCommandEnvelope, isEventEnvelope, parseEventEnvelope } from "../../src/shared/contracts.ts";

const assert = (condition: boolean, message: string) => { if (!condition) throw new Error(message); };
assert(isCommandEnvelope({ version: 1, request_id: "r1", command: { type: "pause" } }), "valid command rejected");
assert(!isEventEnvelope({ version: 2, event_id: "e1", generation: 0, job_id: null, session_id: null, event: { type: "error" } }), "unknown version accepted");
assert(isEventEnvelope({ version: 1, event_id: "root-1", generation: 2, job_id: "library-scan", session_id: null, event: { type: "library_root_status", payload: { root_index: 0, status: "missing" } } }), "root status rejected");
assert(isEventEnvelope({ version: 1, event_id: "shutdown-1", generation: 0, job_id: null, session_id: null, event: { type: "shutdown", payload: { envelope: { version: 1, state: "timed_out", reason: "system" } } } }), "timed out shutdown rejected");
assert(!isEventEnvelope({ version: 1, event_id: "error-1", generation: 2, job_id: null, session_id: null, event: { type: "error", payload: { error: { code: "internal", message: "x" } } } }), "incomplete error accepted");
let rejected = false;
try { parseEventEnvelope(JSON.stringify({ version: 1, event_id: "e1", generation: 0, job_id: null, session_id: null, event: {}, extra: true })); } catch { rejected = true; }
assert(rejected, "malformed event accepted");
