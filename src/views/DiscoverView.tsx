import type { PreviewSnapshot, Recommendation } from "../shared/contracts";
import { previewKey } from "../shared/contracts";

export function DiscoverView({ seed, recommendations, session, loading, error, onRefresh, onTap, onNext, onPrevious, onCancel }: {
  seed: { artist: string; title: string } | null;
  recommendations: Recommendation[];
  session: PreviewSnapshot | null;
  loading: boolean;
  error: string | null;
  onRefresh: () => void;
  onTap: (key: string) => void;
  onNext: () => void;
  onPrevious: () => void;
  onCancel: () => void;
}) {
  const rowState = (key: string, index: number): "current" | "ready" | "unavailable" | "idle" => {
    if (session && session.unavailable.includes(key)) return "unavailable";
    if (session && session.current === key) return "current";
    if (session && session.prepared.includes(key)) return "ready";
    return "idle";
  };
  const stateLabel: Record<ReturnType<typeof rowState>, string> = {
    current: "Previewing",
    ready: "Ready",
    unavailable: "Unavailable",
    idle: "Tap to preview",
  };
  return <section aria-label="Discover" className="luno-discover">
    <h1>Discover</h1>
    <p style={{ color: "var(--luno-text-2)" }}>{seed ? `Recommendations seeded from "${seed.title}" by ${seed.artist}.` : "Select or play a track to seed recommendations."}</p>
    {loading ? <p>Loading recommendations…</p> : null}
    {!loading && error ? <p role="alert" style={{ color: "var(--luno-danger, #E5484D)" }}>{error}</p> : null}
    {!loading && !error && recommendations.length === 0 && !seed ? <p style={{ color: "var(--luno-text-2)" }}>No recommendations yet.</p> : null}
    <ul aria-label="Recommendations" style={{ listStyle: "none", padding: 0, margin: 0 }}>
      {recommendations.map((item, index) => {
        const key = previewKey(item.artist, item.title);
        const state = rowState(key, index);
        const label = stateLabel[state];
        return <li key={key} className="luno-discover-row" style={{ display: "flex", justifyContent: "space-between", alignItems: "center", gap: "0.75rem", padding: "0.5rem 0", borderBottom: "1px solid var(--luno-border, #282828)" }}>
          <span>{item.title}</span>
          <span style={{ color: "var(--luno-text-2)" }}>{item.artist}</span>
          <span aria-label={`${item.title} by ${item.artist}: ${label}`} title={label} style={{ color: "var(--luno-text-2)", minWidth: "5.5rem", textAlign: "right" }}>{label}</span>
          <button type="button" disabled={!seed || loading} onClick={() => onTap(key)}>Tap</button>
        </li>;
      })}
    </ul>
    <div style={{ display: "flex", gap: "0.5rem", marginTop: "0.75rem", alignItems: "center" }}>
      <button type="button" onClick={onRefresh} disabled={!seed || loading}>Refresh</button>
      <button type="button" onClick={onPrevious} disabled={!session || !session.active || session.current === null}>Previous</button>
      <button type="button" onClick={onNext} disabled={!session || !session.active || session.current === null}>Next</button>
      <button type="button" onClick={onCancel} disabled={!session || !session.active}>Cancel</button>
      {session && session.active ? <span style={{ color: "var(--luno-text-2)" }}>{session.prepared.length} of {session.window.length} window items ready</span> : null}
    </div>
    {session && !session.active && session.unavailable.length > 0 ? <p role="alert" style={{ color: "var(--luno-danger, #E5484D)" }}>Preview failed for {session.unavailable.length} recommendation{session.unavailable.length === 1 ? "" : "s"}.</p> : null}
  </section>;
}