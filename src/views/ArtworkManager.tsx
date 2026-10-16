import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { ArtworkCandidate, ArtworkSearchSource, LibrarySnapshot, Track } from "../shared/contracts";
import type { Bridge } from "../app/bridge";

type ArtworkScope = "all" | "favorites" | "unsorted" | string;
type ArtworkBridge = Pick<Bridge, "missingArtworkTracks" | "searchArtworkCandidates" | "searchYouTubeArtworkCandidates" | "applyArtworkCandidate">;

export interface ArtworkManagerProps {
  snapshot?: LibrarySnapshot | null;
  initialScope?: ArtworkScope;
  bridge?: ArtworkBridge;
  onArtworkApplied?: (trackId: string, artworkDataUrl: string) => void;
}

const buttonStyle = { border: 0, borderRadius: 8, padding: "10px 14px", color: "var(--luno-bg)", background: "var(--luno-accent)", fontWeight: 700, cursor: "pointer" } as const;
const secondaryButton = { ...buttonStyle, color: "var(--luno-text)", background: "var(--luno-bg-3)" } as const;
const inputStyle = { width: "100%", padding: "11px 12px", color: "var(--luno-text)", background: "var(--luno-bg-3)", border: "1px solid var(--luno-border)", borderRadius: "var(--luno-radius)" } as const;

function playlistIds(snapshot: LibrarySnapshot | null | undefined, scope: ArtworkScope): string[] {
  const tracks = snapshot?.tracks ?? [];
  if (scope === "all") return tracks.map((track) => track.id);
  if (scope === "favorites") return tracks.filter((track) => track.favorite).map((track) => track.id);
  const playlist = snapshot?.playlists.find((item) => item.id === scope || (scope === "unsorted" && item.name.toLowerCase() === "unsorted"));
  return playlist?.ordered_track_ids.map((id) => id) ?? [];
}

function scopeName(snapshot: LibrarySnapshot | null | undefined, scope: ArtworkScope): string {
  if (scope === "all") return "All music";
  if (scope === "favorites") return "Favorites";
  const playlist = snapshot?.playlists.find((item) => item.id === scope || (scope === "unsorted" && item.name.toLowerCase() === "unsorted"));
  return playlist?.name ?? "Playlist";
}

export function ArtworkManager({ snapshot, initialScope = "all", bridge, onArtworkApplied }: ArtworkManagerProps) {
  const [scope, setScope] = useState<ArtworkScope>(initialScope);
  const [query, setQuery] = useState("");
  const [youtubeQuery, setYoutubeQuery] = useState("");
  const [missingIds, setMissingIds] = useState<string[] | null>(null);
  const [selectedTrackId, setSelectedTrackId] = useState<string | null>(null);
  const selectedTrackIdRef = useRef<string | null>(null);
  const [candidates, setCandidates] = useState<ArtworkCandidate[]>([]);
  const [candidateSearchMode, setCandidateSearchMode] = useState<"automatic" | "manual" | null>(null);
  const [selectedCandidateId, setSelectedCandidateId] = useState<string | null>(null);
  const [scanning, setScanning] = useState(false);
  const [searching, setSearching] = useState(false);
  const [applying, setApplying] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const scanRequestId = useRef(0);
  const tracksById = useMemo(() => new Map((snapshot?.tracks ?? []).map((track) => [track.id, track])), [snapshot?.tracks]);
  const idsForScope = useMemo(() => [...new Set(playlistIds(snapshot, scope))], [snapshot, scope]);
  const scopeTracks = useMemo(() => idsForScope.map((id) => tracksById.get(id)).filter((track): track is Track => !!track), [idsForScope, tracksById]);
  const matchingScopeTracks = useMemo(() => {
    const term = query.trim().toLocaleLowerCase();
    if (!term) return scopeTracks;
    return scopeTracks.filter((track) => `${track.artist ?? ""} ${track.title} ${track.album ?? ""}`.toLocaleLowerCase().includes(term));
  }, [scopeTracks, query]);
  const missingTracks = useMemo(() => (missingIds ?? []).map((id) => tracksById.get(id)).filter((track): track is Track => !!track), [missingIds, tracksById]);
  const filteredMissing = useMemo(() => {
    const term = query.trim().toLocaleLowerCase();
    if (!term) return missingTracks;
    return missingTracks.filter((track) => `${track.artist ?? ""} ${track.title} ${track.album ?? ""}`.toLocaleLowerCase().includes(term));
  }, [missingTracks, query]);
  const selectedTrack = selectedTrackId ? tracksById.get(selectedTrackId) : undefined;

  useEffect(() => {
    setYoutubeQuery(selectedTrack
      ? [selectedTrack.artist, selectedTrack.title].filter((part) => part?.trim()).join(" ")
      : "");
  }, [selectedTrackId, selectedTrack?.artist, selectedTrack?.title]);

  const scan = useCallback(async (trackIds: string[], label: string, replaceResults = false) => {
    if (!bridge) return;
    const requestId = ++scanRequestId.current;
    setScanning(true);
    setError(null);
    setMessage(`Checking ${trackIds.length} song${trackIds.length === 1 ? "" : "s"} in ${label}…`);
    setCandidates([]);
    setCandidateSearchMode(null);
    setSelectedCandidateId(null);
    try {
      const result = await bridge.missingArtworkTracks([...new Set(trackIds)]);
      if (requestId !== scanRequestId.current) return;
      const scannedIds = new Set(trackIds);
      const nextMissingIds = replaceResults
        ? result
        : [...(missingIds ?? []).filter((id) => !scannedIds.has(id)), ...result];
      setMissingIds(nextMissingIds);
      const currentSelection = selectedTrackIdRef.current;
      const nextTrackId = currentSelection && nextMissingIds.includes(currentSelection)
        ? currentSelection
        : nextMissingIds[0] ?? null;
      selectedTrackIdRef.current = nextTrackId;
      setSelectedTrackId(nextTrackId);
      setMessage(result.length === 0
        ? `No songs without embedded artwork among these ${trackIds.length} songs in ${label}.`
        : `${result.length} song${result.length === 1 ? "" : "s"} without embedded artwork among these ${trackIds.length} scanned in ${label}.`);
    } catch (cause) {
      if (requestId !== scanRequestId.current) return;
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      if (requestId === scanRequestId.current) setScanning(false);
    }
  }, [bridge, missingIds]);

  const autoScanKey = JSON.stringify([scope, idsForScope]);
  const lastAutoScannedScope = useRef<string | null>(null);
  useEffect(() => {
    if (!bridge || !snapshot || snapshot.generation <= 0 || lastAutoScannedScope.current === autoScanKey) return;
    lastAutoScannedScope.current = autoScanKey;
    void scan(idsForScope, scopeName(snapshot, scope), true);
  }, [autoScanKey, bridge, idsForScope, scan, scope, snapshot]);

  const search = async (source: ArtworkSearchSource) => {
    if (!bridge || !selectedTrackId) return;
    const requestedTrackId = selectedTrackId;
    setSearching(true);
    setError(null);
    setMessage(null);
    setCandidates([]);
    setCandidateSearchMode("automatic");
    setSelectedCandidateId(null);
    try {
      const result = await bridge.searchArtworkCandidates(requestedTrackId, source);
      if (requestedTrackId !== selectedTrackIdRef.current) return;
      setCandidates(result);
      setSelectedCandidateId(result[0]?.candidate_id ?? null);
      if (result.length === 0) setMessage(source === "youtube"
        ? "No close YouTube matches were found. Try a manual search below with different or shorter terms."
        : "No usable release covers were found. Check the song title and artist, then try again.");
    } catch (cause) {
      if (requestedTrackId === selectedTrackIdRef.current) setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setSearching(false);
    }
  };

  const searchYouTubeManually = async () => {
    if (!bridge || !selectedTrackId || !youtubeQuery.trim()) return;
    const requestedTrackId = selectedTrackId;
    setSearching(true);
    setError(null);
    setMessage(null);
    setCandidates([]);
    setCandidateSearchMode("manual");
    setSelectedCandidateId(null);
    try {
      const result = await bridge.searchYouTubeArtworkCandidates(requestedTrackId, youtubeQuery.trim());
      if (requestedTrackId !== selectedTrackIdRef.current) return;
      setCandidates(result);
      setSelectedCandidateId(result[0]?.candidate_id ?? null);
      setMessage(result.length === 0
        ? "No YouTube video thumbnails were found for that search. Try different or shorter terms."
        : `Showing ${result.length} YouTube video${result.length === 1 ? "" : "s"} in YouTube search order.`);
    } catch (cause) {
      if (requestedTrackId === selectedTrackIdRef.current) setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setSearching(false);
    }
  };

  const apply = async () => {
    if (!bridge || !selectedTrackId || !selectedCandidateId) return;
    const chosen = candidates.find((candidate) => candidate.candidate_id === selectedCandidateId);
    if (!chosen) return;
    setApplying(true);
    setError(null);
    setMessage(null);
    try {
      await bridge.applyArtworkCandidate(selectedTrackId, selectedCandidateId);
      onArtworkApplied?.(selectedTrackId, chosen.artwork_data_url);
      setMissingIds((current) => current?.filter((id) => id !== selectedTrackId) ?? current);
      const nextTrackId = selectedTrackIdRef.current === selectedTrackId ? (missingIds?.find((id) => id !== selectedTrackId) ?? null) : selectedTrackIdRef.current;
      selectedTrackIdRef.current = nextTrackId;
      setSelectedTrackId(nextTrackId);
      setCandidates([]);
      setCandidateSearchMode(null);
      setSelectedCandidateId(null);
      setMessage(`Artwork saved to “${selectedTrack?.title ?? "song"}”.`);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setApplying(false);
    }
  };

  const selectablePlaylists = (snapshot?.playlists ?? []).filter((playlist) => playlist.name.toLowerCase() !== "unsorted");
  return <main className="luno-artwork-manager" aria-labelledby="artwork-manager-heading">
    <header>
      <div>
        <h1 id="artwork-manager-heading">Missing artwork</h1>
        <p>Scan your music files, then review a release match before adding its cover to a song.</p>
      </div>
    </header>
    <section className="luno-artwork-scan" aria-label="Artwork scan">
      <label htmlFor="artwork-scope">Artwork in</label>
      <div className="luno-artwork-scope-row">
        <select id="artwork-scope" value={scope} disabled={scanning || applying} onChange={(event) => {
          setScope(event.target.value);
          setMissingIds(null);
          setSelectedTrackId(null);
          selectedTrackIdRef.current = null;
          setCandidates([]);
          setCandidateSearchMode(null);
          setSelectedCandidateId(null);
          setMessage(null);
          setError(null);
        }} style={inputStyle}>
          <option value="all">All music</option>
          <option value="favorites">Favorites</option>
          {snapshot?.playlists.some((playlist) => playlist.name.toLowerCase() === "unsorted") && <option value="unsorted">Unsorted</option>}
          {selectablePlaylists.map((playlist) => <option key={playlist.id} value={playlist.id}>{playlist.name}</option>)}
        </select>
        <button type="button" onClick={() => scan(idsForScope, scopeName(snapshot, scope), true)} disabled={!bridge || scanning || !snapshot} style={{ ...secondaryButton, opacity: !bridge || scanning || !snapshot ? 0.5 : 1 }}>{scanning ? "Scanning…" : `Scan all ${idsForScope.length} songs`}</button>
      </div>
      <label htmlFor="artwork-filter" style={{ display: "block", marginTop: 16 }}>Find songs to scan</label>
      <input id="artwork-filter" autoCorrect="off" spellCheck={false} value={query} onChange={(event) => setQuery(event.target.value)} placeholder={`Search ${scopeName(snapshot, scope)} by title or artist`} style={{ ...inputStyle, marginTop: 7 }} />
      {query.trim() && <div className="luno-artwork-search-actions">
        <span>{matchingScopeTracks.length} matching song{matchingScopeTracks.length === 1 ? "" : "s"}{matchingScopeTracks.length > 0 && matchingScopeTracks.length <= 3 ? ` · ${matchingScopeTracks.map((track) => track.title).join(", ")}` : ""}</span>
        <button type="button" onClick={() => scan(matchingScopeTracks.map((track) => track.id), `search results in ${scopeName(snapshot, scope)}`)} disabled={!bridge || scanning || matchingScopeTracks.length === 0} style={{ ...buttonStyle, opacity: !bridge || scanning || matchingScopeTracks.length === 0 ? 0.5 : 1 }}>{scanning ? "Scanning…" : `Scan ${matchingScopeTracks.length} match${matchingScopeTracks.length === 1 ? "" : "es"}`}</button>
      </div>}
      <p className="luno-artwork-note">Opening this page scans the selected scope automatically. Search first to scan only matching songs, or rescan the whole scope. YouTube's close-match search uses the saved title and artist; use the manual search to choose your own terms and browse video thumbnails.</p>
    </section>

    {error && <p className="luno-artwork-error" role="alert">{error}</p>}
    {message && <p className="luno-artwork-status" role="status">{message}</p>}

    {missingIds !== null && missingTracks.length > 0 && <div className="luno-artwork-workspace">
      <section className="luno-artwork-missing-list" aria-label="Songs missing artwork">
        <h2>Songs missing artwork <small>({filteredMissing.length} shown)</small></h2>
        <div role="listbox" aria-label="Missing artwork songs">
          {filteredMissing.map((track) => <button key={track.id} type="button" role="option" aria-selected={selectedTrackId === track.id} onClick={() => {
            selectedTrackIdRef.current = track.id;
            setSelectedTrackId(track.id);
            setCandidates([]);
            setCandidateSearchMode(null);
            setSelectedCandidateId(null);
            setMessage(null);
            setError(null);
          }}>
            <strong>{track.title}</strong><span>{track.artist || "Unknown artist"}</span>
          </button>)}
          {filteredMissing.length === 0 && <p>No missing-art songs match that search.</p>}
        </div>
      </section>

      <section className="luno-artwork-review" aria-label="Artwork match review">
        <h2>{selectedTrack ? `${selectedTrack.artist ? `${selectedTrack.artist} · ` : ""}${selectedTrack.title}` : "Choose a song"}</h2>
        {selectedTrack && <>
          <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
            <button type="button" onClick={() => search("musicbrainz")} disabled={!bridge || searching || applying} style={{ ...buttonStyle, opacity: !bridge || searching || applying ? 0.5 : 1 }}>
              {searching ? "Searching…" : "Find artwork matches"}
            </button>
            <button type="button" onClick={() => search("youtube")} disabled={!bridge || searching || applying} style={{ ...secondaryButton, opacity: !bridge || searching || applying ? 0.5 : 1 }}>
              {searching ? "Searching…" : "Find close YouTube matches"}
            </button>
          </div>
          <form className="luno-artwork-youtube-search" onSubmit={(event) => { event.preventDefault(); void searchYouTubeManually(); }}>
            <label htmlFor="artwork-youtube-query">Search YouTube videos</label>
            <div>
              <input id="artwork-youtube-query" value={youtubeQuery} onChange={(event) => setYoutubeQuery(event.target.value)} maxLength={512} placeholder="Search by song, artist, or video title" style={inputStyle} />
              <button type="submit" disabled={!bridge || searching || applying || !youtubeQuery.trim()} style={{ ...secondaryButton, opacity: !bridge || searching || applying || !youtubeQuery.trim() ? 0.5 : 1 }}>{searching ? "Searching…" : "Search YouTube"}</button>
            </div>
          </form>
          {!selectedTrack.artist?.trim() && <p className="luno-artwork-note">No artist name is saved, so searches use the title only. Review matches carefully.</p>}
        </>}
        {candidates.length > 0 && <>
          <p className="luno-artwork-note">{candidates[0]?.source === "youtube"
            ? candidateSearchMode === "manual"
              ? "Select the video whose thumbnail you want, then use it for this song. Results follow YouTube's search order."
              : "These are video thumbnails and may not be the album cover. Choose a close match and review it before applying."
            : "Choose the cover that matches this recording. MusicBrainz scores sort results; check the artist and release before applying."}</p>
          <div className={`luno-artwork-candidates${candidates[0]?.source === "youtube" ? " luno-artwork-candidates-youtube" : ""}`} role="radiogroup" aria-label="Artwork matches">
            {candidates.map((candidate) => <button key={candidate.candidate_id} type="button" role="radio" aria-checked={selectedCandidateId === candidate.candidate_id} onClick={() => setSelectedCandidateId(candidate.candidate_id)}>
              <img src={candidate.artwork_data_url} alt="" />
              <span><strong>{candidate.release}</strong><small>{candidate.artist || "Unknown artist"}{candidate.year ? ` · ${candidate.year}` : ""}</small><small>{candidate.source === "youtube" ? "YouTube video thumbnail" : candidate.exact_match ? "Track title and artist match" : `MusicBrainz score ${candidate.score}`}</small></span>
            </button>)}
          </div>
          <button type="button" onClick={apply} disabled={!selectedCandidateId || applying} style={{ ...buttonStyle, marginTop: 12, opacity: !selectedCandidateId || applying ? 0.5 : 1 }}>{applying ? "Saving artwork…" : candidateSearchMode === "manual" ? "Use this thumbnail" : "Apply selected artwork"}</button>
        </>}
        <p className="luno-artwork-source">Release covers come from MusicBrainz and Cover Art Archive. YouTube results are video thumbnails, which may not be album art. Review every image before embedding. Unsupported formats stay unchanged.</p>
      </section>
    </div>}
  </main>;
}
