import type { PlaylistSyncReviewSummary } from "../shared/contracts";

export function PlaylistSyncReviewView({ entries, loading, error, onOpenReview, onRefresh }: {
  entries: PlaylistSyncReviewSummary[];
  loading: boolean;
  error: string | null;
  onOpenReview: (playlistId: PlaylistSyncReviewSummary["playlist_id"]) => void;
  onRefresh: () => void;
}) {
  return <section className="luno-sync-review-settings" aria-labelledby="playlist-sync-review-title">
    <header>
      <div>
        <h2 id="playlist-sync-review-title">Playlist Sync Review</h2>
        <p>Review unavailable YouTube songs, match them to music already in your library, or dismiss entries you no longer want to sync.</p>
      </div>
      <button type="button" onClick={onRefresh} disabled={loading}>Refresh</button>
    </header>
    {loading && <p role="status">Checking playlists…</p>}
    {error && <p role="alert" className="luno-sync-review-settings-error">{error}</p>}
    {!loading && !error && entries.length === 0 && <p className="luno-sync-review-settings-empty">No playlist sync issues need review. Linked playlists with unresolved songs will appear here after a sync.</p>}
    <div className="luno-sync-review-settings-list">
      {entries.map((entry) => <article key={entry.playlist_id}>
        <div>
          <h3>{entry.name}</h3>
          <p>{entry.url ? "YouTube playlist linked" : "Link removed"}</p>
          <span>{entry.unresolved_count > 0
            ? `${entry.unresolved_count} unavailable ${entry.unresolved_count === 1 ? "song" : "songs"}`
            : "No unavailable songs"}</span>
        </div>
        <button type="button" onClick={() => onOpenReview(entry.playlist_id)} disabled={entry.unresolved_count === 0}>
          {entry.unresolved_count > 0 ? `Review ${entry.unresolved_count} ${entry.unresolved_count === 1 ? "song" : "songs"}` : "No review needed"}
        </button>
      </article>)}
    </div>
  </section>;
}
