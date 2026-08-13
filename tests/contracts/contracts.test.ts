import { isCommandEnvelope, isEventEnvelope, parseEventEnvelope } from "../../src/shared/contracts.ts";

const assert = (condition: boolean, message: string) => { if (!condition) throw new Error(message); };
assert(isCommandEnvelope({ version: 1, request_id: "r1", command: { type: "pause" } }), "valid command rejected");
assert(!isEventEnvelope({ version: 2, event_id: "e1", generation: 0, job_id: null, session_id: null, event: { type: "error" } }), "unknown version accepted");
let rejected = false;
try { parseEventEnvelope(JSON.stringify({ version: 1, event_id: "e1", generation: 0, job_id: null, session_id: null, event: {}, extra: true })); } catch { rejected = true; }
assert(rejected, "malformed event accepted");
