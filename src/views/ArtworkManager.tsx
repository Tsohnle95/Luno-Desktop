import { useEffect, useMemo, useRef, useState } from "react";
import type { ArtworkCandidate, LibrarySnapshot, Track } from "../shared/contracts";
import type { Bridge } from "../app/bridge";

type ArtworkScope = "all" | "favorites" | "unsorted" | string;
type ArtworkBridge = Pick<Bridge, "missingArtworkTracks" | "searchArtworkCandidates" | "applyArtworkCandidate">;

export interface ArtworkManagerProps {
  snapshot?: LibrarySnapshot | null;
  initialScope?: ArtworkScope;
  bridge?: ArtworkBridge;
  onArtworkApplied?: (trackId: string, artworkDataUrl: string) => void;
}

const buttonStyle = { border: 0, borderRadius: 8, padding: "10px 14px", color: "var(--luno-bg)", background: "var(--luno-green)", fontWeight: 700, cursor: "pointer" } as const;
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
  const [missingIds, setMissingIds] = useState<string[] | null>(null);
  const [selectedTrackId, setSelectedTrackId] = useState<string | null>(null);
  const selectedTrackIdRef = useRef<string | null>(null);
  const [candidates, setCandidates] = useState<ArtworkCandidate[]>([]);
  const [selectedCandidateId, setSelectedCandidateId] = useState<string | null>(null);
  const [scanning, setScanning] = useState(false);
  const [searching, setSearching] = useState(false);
  const [applying, setApplying] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const tracksById = useMemo(() => new Map((snapshot?.tracks ?? []).map((track) => [track.id, track])), [snapshot?.tracks]);
  const idsForScope = useMemo(() => [...new Set(playlistIds(snapshot, scope))], [snapshot, scope]);
  const missingTracks = useMemo(() => (missingIds ?? []).map((id) => tracksById.get(id)).filter((track): track is Track => !!track), [missingIds, tracksById]);
  const filteredMissing = useMemo(() => {
    const term = query.trim().toLocaleLowerCase();
    if (!term) return missingTracks;
    return missingTracks.filter((track) => `${track.artist ?? ""} ${track.title} ${track.album ?? ""}`.toLocaleLowerCase().includes(term));
  }, [missingTracks, query]);
  const selectedTrack = selectedTrackId ? tracksById.get(selectedTrackId) : undefined;

  useEffect(() => {
    setScope(initialScope);
    setMissingIds(null);
    setSelectedTrackId(null);
    selectedTrackIdRef.current = null;
    setCandidates([]);
    setSelectedCandidateId(null);
    setQuery("");
    setMessage(null);
    setError(null);
  }, [initialScope]);

  const scan = async () => {
    if (!bridge) return;
    setScanning(true);
    setError(null);
    setMessage(null);
    setCandidates([]);
    setSelectedCandidateId(null);
    try {
      const result = await bridge.missingArtworkTracks(idsForScope);
      setMissingIds(result);
      const nextTrackId = result.includes(selectedTrackIdRef.current ?? "") ? selectedTrackIdRef.current : result[0] ?? null;
      selectedTrackIdRef.current = nextTrackId;
      setSelectedTrackId(nextTrackId);
      setMessage(result.length === 0 ? `No songs without embedded artwork in ${scopeName(snapshot, scope)}.` : `${result.length} song${result.length === 1 ? "" : "s"} without embedded artwork.`);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setScanning(false);
    }
  };

  const search = async () => {
    if (!bridge || !selectedTrackId) return;
    const requestedTrackId = selectedTrackId;
    setSearching(true);
    setError(null);
    setMessage(null);
    setCandidates([]);
    setSelectedCandidateId(null);
    try {
      const result = await bridge.searchArtworkCandidates(requestedTrackId);
      if (requestedTrackId !== selectedTrackIdRef.current) return;
      setCandidates(result);
      setSelectedCandidateId(result[0]?.candidate_id ?? null);
      if (result.length === 0) setMessage("No usable cover matches were found. Check the song title and artist, then try again.");
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
      setSelectedCandidateId(null);
      setMessage(`Artwork saved to “${selectedTrack?.title ?? "song"}”.`);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setApplying(false);
    }
  };

  const selectablePlaylists = (snapshot?.playlists ?? []).filter((playlist) => playlist.name.toLowerCase() !== "unsorted");
  const selectedHasArtist = !!selectedTrack?.artist?.trim();

  return <main className="luno-artwork-manager" aria-labelledby="artwork-manager-heading">
    <header>
      <div>
        <h1 id="artwork-manager-heading">Missing artwork</h1>
        <p>Scan your music files, then review a release match before adding its cover to a song.</p>
      </div>
    </header>
    <section className="luno-artwork-scan" aria-label="Artwork scan">
      <label htmlFor="artwork-scope">Scan</label>
      <div className="luno-artwork-scope-row">
        <select id="artwork-scope" value={scope} disabled={scanning || applying} onChange={(event) => {
          setScope(event.target.value);
          setMissingIds(null);
          setSelectedTrackId(null);
          selectedTrackIdRef.current = null;
          setCandidates([]);
          setSelectedCandidateId(null);
          setMessage(null);
          setError(null);
        }} style={inputStyle}>
          <option value="all">All music</option>
          <option value="favorites">Favorites</option>
          {snapshot?.playlists.some((playlist) => playlist.name.toLowerCase() === "unsorted") && <option value="unsorted">Unsorted</option>}
          {selectablePlaylists.map((playlist) => <option key={playlist.id} value={playlist.id}>{playlist.name}</option>)}
        </select>
        <button type="button" onClick={scan} disabled={!bridge || scanning || !snapshot} style={{ ...buttonStyle, opacity: !bridge || scanning || !snapshot ? 0.5 : 1 }}>{scanning ? "Scanning…" : "Scan for missing artwork"}</button>
      </div>
      <p className="luno-artwork-note">The scan checks embedded audio artwork locally. Online search sends the selected song’s title and artist to MusicBrainz.</p>
    </section>

    {error && <p className="luno-artwork-error" role="alert">{error}</p>}
    {message && <p className="luno-artwork-status" role="status">{message}</p>}

    {missingIds !== null && missingTracks.length > 0 && <div className="luno-artwork-workspace">
      <section className="luno-artwork-missing-list" aria-label="Songs missing artwork">
        <label htmlFor="artwork-filter">Find a song</label>
        <input id="artwork-filter" autoCorrect="off" spellCheck={false} value={query} onChange={(event) => setQuery(event.target.value)} placeholder="Type a title or artist" style={inputStyle} />
        <div role="listbox" aria-label="Missing artwork songs">
          {filteredMissing.map((track) => <button key={track.id} type="button" role="option" aria-selected={selectedTrackId === track.id} onClick={() => {
            selectedTrackIdRef.current = track.id;
            setSelectedTrackId(track.id);
            setCandidates([]);
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
          <button type="button" onClick={search} disabled={!bridge || searching || applying || !selectedHasArtist} style={{ ...buttonStyle, opacity: !bridge || searching || applying || !selectedHasArtist ? 0.5 : 1 }}>
            {searching ? "Searching…" : "Find artwork matches"}
          </button>
          {!selectedHasArtist && <p className="luno-artwork-note">Add an artist name to search for a matching release.</p>}
        </>}
        {candidates.length > 0 && <>
          <p className="luno-artwork-note">Choose the cover that matches this recording. MusicBrainz scores help sort results; check the artist and release before applying.</p>
          <div className="luno-artwork-candidates" role="radiogroup" aria-label="Artwork matches">
            {candidates.map((candidate) => <button key={candidate.candidate_id} type="button" role="radio" aria-checked={selectedCandidateId === candidate.candidate_id} onClick={() => setSelectedCandidateId(candidate.candidate_id)}>
              <img src={candidate.artwork_data_url} alt="" />
              <span><strong>{candidate.release}</strong><small>{candidate.artist || "Unknown artist"}{candidate.year ? ` · ${candidate.year}` : ""}</small><small>{candidate.exact_match ? "Track title and artist match" : `MusicBrainz score ${candidate.score}`}</small></span>
            </button>)}
          </div>
          <button type="button" onClick={apply} disabled={!selectedCandidateId || applying} style={{ ...buttonStyle, marginTop: 12, opacity: !selectedCandidateId || applying ? 0.5 : 1 }}>{applying ? "Saving artwork…" : "Apply selected artwork"}</button>
        </>}
        <p className="luno-artwork-source">Matches come from MusicBrainz and Cover Art Archive. Check the match and respect artist and label rights before embedding. Unsupported formats stay unchanged.</p>
      </section>
    </div>}
  </main>;
}
