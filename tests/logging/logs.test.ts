import { isLogEntry, isLogEvent, projectErrorLog, MAX_LOG_ENTRIES, type LogEntry } from "../../src/shared/logs";

const entry = (index: number): LogEntry => ({ timestamp_ms: index, level: "error", area: "worker", message: `failure ${index}`, error_kind: "worker" });

const entries = Array.from({ length: MAX_LOG_ENTRIES + 1 }, (_, index) => entry(index));
const projection = projectErrorLog(entries);
if (projection.entries.length !== MAX_LOG_ENTRIES || projection.entries[0].timestamp_ms !== 1) throw new Error("Error Log projection must be bounded to 500 entries");
if (!isLogEntry(projection.entries[0])) throw new Error("valid log entry rejected");
if (!isLogEvent({ type: "added", payload: { entry: projection.entries[0] } }) || !isLogEvent({ type: "shutdown" })) throw new Error("log event rejected");
