import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { DownloaderView } from "../../views/DownloaderView";

const runningJob = { id: "job-1", kind: "single" as const, state: "running" as const, track_id: null, progress: 0.5, generation: 1 };

describe("Gate 6 downloader view", () => {
  it("renders disclosure buttons with aria-expanded instead of native details", () => {
    const html = renderToStaticMarkup(<DownloaderView />);
    expect(html).not.toContain("<details");
    expect(html).not.toContain("<summary");
    expect(html).toContain('aria-expanded="true"');
    expect(html).toContain('aria-expanded="false"');
    expect(html).toContain("Progress");
    expect(html).toContain("Download log");
    expect(html).toContain("Downloaded songs");
  });

  it("renders tablist with seven wired tabs and an explicit panel", () => {
    const html = renderToStaticMarkup(<DownloaderView />);
    expect(html).toContain('role="tablist"');
    expect(html).toContain('role="tabpanel"');
    expect((html.match(/role="tab"/g) ?? []).length).toBe(7);
    expect(html).toContain('aria-controls="download-panel-search"');
    expect(html).toContain('aria-labelledby="download-tab-search"');
  });

  it("keeps only the active tab in the tab order (roving tabindex)", () => {
    const html = renderToStaticMarkup(<DownloaderView />);
    expect((html.match(/id="download-tab-[a-z-]+" tabindex="0"/g) ?? [])).toHaveLength(1);
    expect((html.match(/id="download-tab-[a-z-]+" tabindex="-1"/g) ?? [])).toHaveLength(6);
    expect(html).toContain('id="download-tab-search" tabindex="0"');
  });

  it("exposes progress, cancel, and live-region affordances for active jobs", () => {
    const html = renderToStaticMarkup(<DownloaderView jobs={{ "job-1": runningJob }} />);
    expect(html).toContain("Downloading — 50%");
    expect(html).toContain("Cancel");
    expect(html).toContain('aria-live="polite"');
  });
});