import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { DownloaderView } from "../views/DownloaderView";
import { SettingsView } from "../views/SettingsView";
import { EditTrackDialog } from "../components/EditTrackDialog";
import { ConfirmDialog } from "../components/ConfirmDialog";
import { PlayerBar } from "../components/PlayerBar";
import { PlaylistUrlDialog, PlaylistSyncAuthDialog, PlaylistSyncLogDialog, PlaylistSyncErrorsDialog, PlaylistSyncReviewDialog } from "../components/PlaylistDialogs";

describe("Gate 4 views", () => {
  it("renders downloader error, cancel, and retry controls", () => {
    const html = renderToStaticMarkup(<DownloaderView error="helper failed" jobs={{ one: { id: "one", kind: "single", state: "failed", track_id: null, progress: null, generation: 1 }, two: { id: "two", kind: "single", state: "running", track_id: null, progress: 0.5, generation: 1 } }} />);
    expect(html).toContain("helper failed");
    expect(html).toContain("Cancel");
    expect(html).toContain("Retry");
  });

  it("exposes export, preview, apply, and unavailable-root feedback", () => {
    const html = renderToStaticMarkup(<SettingsView error="Native library root picker is unavailable in this runtime." transferManifest="{}" transferDestination="/fixture/library" transferResult="matched: 1" onTransferExport={() => {}} onTransferPreview={() => {}} onTransferApply={() => {}} />);
    expect(html).toContain("Export current library");
    expect(html).toContain("Preview import");
    expect(html).toContain("Apply approved import");
    expect(html).toContain("Native library root picker is unavailable");
    expect(html).toContain("matched: 1");
  });
});

describe("Edit track dialog", () => {
  it("prefills the current artist and title for correction", () => {
    const html = renderToStaticMarkup(<EditTrackDialog track={{ title: "Song", artist: "Artist" }} onClose={() => {}} onSubmit={() => {}} />);
    expect(html).toContain("Edit song info");
    expect(html).toContain('value="Song"');
    expect(html).toContain('value="Artist"');
    expect(html).toContain("Luno reads artist and title from the file name");
  });

  it("surfaces a rename failure without closing the dialog", () => {
    const html = renderToStaticMarkup(<EditTrackDialog track={{ title: "Song", artist: null }} error="a file with that name already exists" onClose={() => {}} onSubmit={() => {}} />);
    expect(html).toContain('role="alert"');
    expect(html).toContain("a file with that name already exists");
  });
});

describe("Confirm dialog", () => {
  it("renders a modal with the destructive action and a cancel path", () => {
    const html = renderToStaticMarkup(<ConfirmDialog title="Delete from library" message="Permanently delete “Song” from your library?" confirmLabel="Delete from library" danger onClose={() => {}} onConfirm={() => {}} />);
    expect(html).toContain('role="dialog"');
    expect(html).toContain('aria-modal="true"');
    expect(html).toContain("Permanently delete “Song” from your library?");
    expect(html).toContain("Delete from library");
    expect(html).toContain("Cancel");
    // last-child footer button carries the danger class (styled red)
    expect(html).toMatch(/class="is-danger"[^>]*>Delete from library</);
  });
});

describe("Player bar context menu", () => {
  const track = { id: "t1", path: null, title: "Song", artist: "Artist", album: null, duration_ms: 1000, artwork_url: null, favorite: false, provider: null };
  it("renders the now-playing track and accepts a context-menu handler", () => {
    const html = renderToStaticMarkup(<PlayerBar track={track} onTrackContextMenu={() => {}} />);
    expect(html).toContain("Song");
    expect(html).toContain("Artist");
  });
});

describe("YouTube playlist URL dialog", () => {
  it("explains new-only sync and offers removal for an existing link", () => {
    const html = renderToStaticMarkup(<PlaylistUrlDialog playlistName="Road trip" initialUrl="https://www.youtube.com/playlist?list=PL123" onClose={() => {}} onSubmit={() => {}} />);
    expect(html).toContain("YouTube playlist sync");
    expect(html).toContain("only songs Luno has not already seen");
    expect(html).toContain("Remove link");
    expect(html).toContain("PL123");
  });

  it("requests a signed-in browser inside the playlist sync flow", () => {
    const html = renderToStaticMarkup(<PlaylistSyncAuthDialog playlistName="Liked songs" onClose={() => {}} onSubmit={() => {}} />);
    expect(html).toContain("Sign in to sync this playlist");
    expect(html).toContain("Choose a browser signed in to the account that owns this playlist");
    expect(html).toContain('id="playlist-sync-browser"');
    expect(html).toContain("Firefox");
    expect(html).toContain("Save browser and sync");
    expect(html).toContain("Luno saves the browser choice, not a cookie file.");
  });
});

describe("Playlist sync history", () => {
  const entries = [
    { title: "Artist - New song", video_id: "video-1", at_ms: 1_700_000_000_000, status: "downloaded", reason: null },
    { title: "Artist - Unavailable song", video_id: "video-2", at_ms: 1_700_000_000_001, status: "failed", reason: "video unavailable" },
  ];
  it("shows only dated successful additions in the playlist log", () => {
    const html = renderToStaticMarkup(<PlaylistSyncLogDialog playlistName="Road trip" loading={false} error={null} onClose={() => {}} entries={entries} />);
    expect(html).toContain("Sync log · Road trip");
    expect(html).toContain("Artist - New song");
    expect(html).not.toContain("Artist - Unavailable song");
  });
  it("shows failed songs in a separate error dialog", () => {
    const html = renderToStaticMarkup(<PlaylistSyncErrorsDialog playlistName="Road trip" loading={false} error={null} onClose={() => {}} entries={entries.filter((entry) => entry.status === "failed")} />);
    expect(html).toContain("Sync errors · Road trip");
    expect(html).toContain("Artist - Unavailable song");
    expect(html).toContain("video unavailable");
  });

  it("shows original review cues and requires an explicit candidate choice", () => {
    const html = renderToStaticMarkup(<PlaylistSyncReviewDialog playlistName="Road trip" loading={false} error={null}
      entries={[{ video_id: "aPSOTX5AkiY", title: "Get What You Give", channel: "Boogie T - Topic", duration_secs: 292, reason: "Video unavailable", attempted_source_id: "aPSOTX5AkiY", selected_source_id: null, retry_pending: false, updated_at_ms: 1, local_matches: [] }]}
      candidateLookups={{ aPSOTX5AkiY: { loading: false, error: null, candidates: [{ video_id: "6U7_Qy5c_uc", title: "Get What You Give", channel: "Boogie T - Topic", duration: "4:51", thumbnail_url: null, url: "https://www.youtube.com/watch?v=6U7_Qy5c_uc" }] }}}
      onFindCandidates={() => {}} onChooseCandidate={() => {}} onRetryOriginal={() => {}} onSyncSelected={() => {}} onResolveWithLocalMatch={() => {}} onDismissIssue={() => {}} onClose={() => {}} />);
    expect(html).toContain("Review unavailable songs · Road trip");
    expect(html).toContain("Boogie T - Topic · 4:52 original length · original video unavailable");
    expect(html).toContain("Get What You Give");
    expect(html).toContain("4:51");
    expect(html).toContain("Download this version to playlist");
    expect(html).toContain("Retry original source");
    expect(html).not.toContain("Sync selected sources");
    expect(html).not.toContain("selected=");

    const pendingHtml = renderToStaticMarkup(<PlaylistSyncReviewDialog playlistName="Road trip" loading={false} error={null}
      entries={[{ video_id: "aPSOTX5AkiY", title: "Get What You Give", channel: "Boogie T - Topic", duration_secs: 292, reason: "Video unavailable", attempted_source_id: "6U7_Qy5c_uc", selected_source_id: "6U7_Qy5c_uc", retry_pending: true, updated_at_ms: 1, local_matches: [] }]}
      onFindCandidates={() => {}} onChooseCandidate={() => {}} onRetryOriginal={() => {}} onSyncSelected={() => {}} onResolveWithLocalMatch={() => {}} onDismissIssue={() => {}} onClose={() => {}} />);
    expect(pendingHtml).toContain("Selected source: 6U7_Qy5c_uc · ready for next sync");
    expect(pendingHtml).toContain("Sync selected sources (1)");

    const selectingHtml = renderToStaticMarkup(<PlaylistSyncReviewDialog playlistName="Road trip" loading={false} error={null} selectingVideoId="aPSOTX5AkiY"
      entries={[{ video_id: "aPSOTX5AkiY", title: "Get What You Give", channel: "Boogie T - Topic", duration_secs: 292, reason: "Video unavailable", attempted_source_id: "6U7_Qy5c_uc", selected_source_id: "6U7_Qy5c_uc", retry_pending: true, updated_at_ms: 1, local_matches: [] }]}
      onFindCandidates={() => {}} onChooseCandidate={() => {}} onRetryOriginal={() => {}} onSyncSelected={() => {}} onResolveWithLocalMatch={() => {}} onDismissIssue={() => {}} onClose={() => {}} />);
    expect(selectingHtml).toMatch(/disabled=""[^>]*>Sync selected sources \(1\)/);

    const failedHtml = renderToStaticMarkup(<PlaylistSyncReviewDialog playlistName="Road trip" loading={false} error={null}
      entries={[{ video_id: "aPSOTX5AkiY", title: "Get What You Give", channel: "Boogie T - Topic", duration_secs: 292, reason: "Video unavailable", attempted_source_id: "6U7_Qy5c_uc", selected_source_id: "6U7_Qy5c_uc", retry_pending: false, updated_at_ms: 1, local_matches: [] }]}
      onFindCandidates={() => {}} onChooseCandidate={() => {}} onRetryOriginal={() => {}} onSyncSelected={() => {}} onResolveWithLocalMatch={() => {}} onDismissIssue={() => {}} onClose={() => {}} />);
    expect(failedHtml).toContain("Selected source: 6U7_Qy5c_uc · last attempt finished; select again to retry or change");
    expect(failedHtml).toContain("Last unavailable attempt: 6U7_Qy5c_uc");
    expect(failedHtml).toContain("Video unavailable");
    expect(failedHtml).toContain("Search available versions");
    expect(failedHtml).toContain("Retry original source");
  });
});
