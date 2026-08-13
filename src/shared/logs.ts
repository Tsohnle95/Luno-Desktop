export const MAX_LOG_ENTRIES = 500 as const;
export type LogLevel = "debug" | "info" | "warn" | "error";
export type LogErrorKind = "invalid_request" | "not_found" | "busy" | "cancelled" | "internal" | "shutting_down" | "worker";
export interface LogEntry {
  timestamp_ms: number;
  level: LogLevel;
  area: string;
  message: string;
  error_kind: LogErrorKind | null;
}
export type LogEvent = { type: "added"; payload: { entry: LogEntry } } | { type: "shutdown" };
export interface ErrorLogProjection { entries: LogEntry[]; max_entries: typeof MAX_LOG_ENTRIES; closed: boolean }
export function projectErrorLog(entries: readonly LogEntry[], closed = false): ErrorLogProjection {
  return { entries: entries.slice(-MAX_LOG_ENTRIES), max_entries: MAX_LOG_ENTRIES, closed };
}
export function isLogEntry(value: unknown): value is LogEntry {
  if (typeof value !== "object" || value === null) return false;
  const entry = value as Record<string, unknown>;
  return typeof entry.timestamp_ms === "number" && ["debug", "info", "warn", "error"].includes(String(entry.level)) && typeof entry.area === "string" && typeof entry.message === "string" && (entry.error_kind === null || typeof entry.error_kind === "string");
}
export function isLogEvent(value: unknown): value is LogEvent {
  if (typeof value !== "object" || value === null) return false;
  const event = value as Record<string, unknown>;
  return event.type === "shutdown" || (event.type === "added" && typeof event.payload === "object" && event.payload !== null && isLogEntry((event.payload as Record<string, unknown>).entry));
}
