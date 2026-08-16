import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { DownloaderView } from "../views/DownloaderView";
import { SettingsView } from "../views/SettingsView";

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
