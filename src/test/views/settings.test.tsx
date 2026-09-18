import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { SettingsView } from "../../views/SettingsView";

describe("Settings library root", () => {
  it("renders the editable production folder input when onLibraryRootSubmit is provided", () => {
    const html = renderToStaticMarkup(<SettingsView libraryRootPath="/Users/me/Music" onLibraryRootSubmit={() => {}} />);
    expect(html).toContain('id="music-root"');
    expect(html).toContain('value="/Users/me/Music"');
    expect(html).toContain("Set folder");
    expect(html).not.toContain("Change");
  });

  it("links the production folder input to the submit callback aria and helper text", () => {
    const html = renderToStaticMarkup(<SettingsView onLibraryRootSubmit={() => {}} />);
    expect(html).toContain('aria-describedby="music-root-error"');
    expect(html).toContain("Enter an absolute folder path; Luno scans it as your music library and downloads land inside it.");
  });

  it("keeps the dev read-only change row when only onChooseMusicRoot is provided", () => {
    const html = renderToStaticMarkup(<SettingsView musicRoot="Music" onChooseMusicRoot={() => {}} />);
    expect(html).toContain("Change");
    expect(html).not.toContain("Set folder");
    expect(html).toContain('value="Music"');
  });

  it("disables OS autocorrect on the editable folder field", () => {
    const html = renderToStaticMarkup(<SettingsView onLibraryRootSubmit={() => {}} />);
    expect(html.toLowerCase()).toContain('autocorrect="off"');
    expect(html.toLowerCase()).toContain('spellcheck="false"');
  });
});

describe("Settings navigation rail", () => {
  it("exposes home, app destinations, and every settings category as flat tabs", () => {
    const html = renderToStaticMarkup(<SettingsView />);
    expect(html).toContain("Home");
    expect(html).toContain("Recent Songs");
    expect(html).toContain("Discover");
    expect(html).toContain("Downloader");
    expect(html).toContain("Error Log");
    expect(html).toContain("Playback");
    expect(html).toContain("Transfer");
    expect(html).toContain("Troubleshooting");
    expect(html).not.toContain("Go to");
    expect(html).not.toContain("Song Page");
    expect(html).toContain('aria-label="Settings"');
  });

  it("renders an embedded rail screen in place of the settings panels", () => {
    const html = renderToStaticMarkup(<SettingsView screen={<div>Embedded screen</div>} activeScreen="recent" />);
    expect(html).toContain("Embedded screen");
    expect(html).not.toContain("Storage &amp; Library");
    expect((html.match(/aria-current="true"/g) ?? []).length).toBe(1);
  });
});