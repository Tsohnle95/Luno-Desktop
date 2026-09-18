import { describe, expect, it } from "vitest";
import { downloadDisplayLabel, downloadFilename } from "../state/download_naming";

describe("download naming", () => {
  it("builds Artist - Title filenames from resolved metadata", () => {
    expect(downloadFilename({ artist: "Rick Astley", title: "Never Gonna Give You Up" }, "m4a", "dl-1"))
      .toBe("Rick Astley - Never Gonna Give You Up.m4a");
  });

  it("falls back to the supplied stem when metadata is missing", () => {
    expect(downloadFilename({}, "m4a", "dl-1789755401728-httpswwwyoutubecomwatchv"))
      .toBe("dl-1789755401728-httpswwwyoutubecomwatchv.m4a");
    expect(downloadFilename({ artist: "  ", title: "" }, "m4a", "fallback")).toBe("fallback.m4a");
  });

  it("strips path separators and control characters from filenames", () => {
    expect(downloadFilename({ artist: "AC/DC", title: "Back:In\\Black?*" }, "m4a", "x"))
      .toBe("ACDC - BackInBlack.m4a");
  });

  it("renders a readable display label only when metadata exists", () => {
    expect(downloadDisplayLabel({ artist: "Rick Astley", title: "Never Gonna Give You Up" })).toBe("Rick Astley — Never Gonna Give You Up");
    expect(downloadDisplayLabel({ title: "Untitled" })).toBe("Untitled");
    expect(downloadDisplayLabel({})).toBeNull();
  });
});
