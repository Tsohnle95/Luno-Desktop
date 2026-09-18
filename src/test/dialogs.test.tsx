import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { DownloaderView } from "../views/DownloaderView";
import { SettingsView } from "../views/SettingsView";
import { EditTrackDialog } from "../components/EditTrackDialog";
import { ConfirmDialog } from "../components/ConfirmDialog";
import { PlayerBar } from "../components/PlayerBar";

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
