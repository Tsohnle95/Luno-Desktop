import { projectErrorLog, type LogEntry } from "../shared/logs";

export interface ErrorLogViewProps { entries?: readonly LogEntry[]; closed?: boolean; onClear?: () => void; loading?: boolean; error?: string | null; }

function redact(value: string): string {
  return value.replace(/(api[_ -]?key|token|secret|password)(\s*[=:]\s*)[^\s,;]+/gi, "$1$2[REDACTED]")
    .replace(/https?:\/\/[^\s]+/gi, "[REDACTED_URL]");
}

function formatEntry(entry: LogEntry): string {
  const time = new Date(entry.timestamp_ms).toLocaleTimeString();
  return `[${time}] ${entry.level.toUpperCase()} ${entry.area}: ${redact(entry.message)}${entry.error_kind ? ` (${entry.error_kind})` : ""}`;
}

export function ErrorLogView({ entries = [], closed = false, onClear, loading = false, error = null }: ErrorLogViewProps) {
  const projection = projectErrorLog(entries, closed);
  return <div style={{ minHeight: "100%", padding: "36px clamp(20px, 5vw, 48px) 40px", display: "flex", flexDirection: "column" }}>
    <header style={{ display: "flex", justifyContent: "space-between", alignItems: "center", gap: 16, marginBottom: 22 }}><div><p style={{ color: "var(--luno-text-2)", margin: 0 }}>Diagnostics and recovery information</p><h1 style={{ fontSize: 30, margin: "8px 0 0" }}>Error &amp; Debug Logs</h1></div><button type="button" onClick={onClear} disabled={!projection.entries.length} style={{ border: 0, background: "transparent", color: "var(--luno-text-2)", cursor: projection.entries.length ? "pointer" : "default", padding: 10 }}>Clear logs</button></header>
    {closed && <p role="status" style={{ color: "var(--luno-warning)" }}>Logging has stopped because the application is shutting down.</p>}
    {loading && <p role="status">Loading logs...</p>}
    {error && <p role="alert" style={{ color: "var(--luno-red)" }}>{error}</p>}
    <div role="log" aria-label="Application error log" aria-live="polite" style={{ flex: 1, minHeight: 240, maxHeight: "calc(100vh - 220px)", overflow: "auto", whiteSpace: "pre-wrap", overflowWrap: "anywhere", background: "var(--luno-bg)", border: "1px solid var(--luno-border)", borderRadius: 8, padding: 16, fontFamily: "var(--luno-font-mono)", color: "var(--luno-text-2)", lineHeight: 1.55 }}>{projection.entries.length ? projection.entries.map((entry, index) => <div key={`${entry.timestamp_ms}-${index}`}>{formatEntry(entry)}</div>) : "--- No logs yet ---"}</div>
    <small style={{ color: "var(--luno-text-3)", marginTop: 10 }}>{projection.entries.length}/{projection.max_entries} entries shown. Secrets and URLs are redacted.</small>
  </div>;
}
