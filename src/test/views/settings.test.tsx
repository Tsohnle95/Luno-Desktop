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
});