import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { VideoStage, ringDashOffset, endedHoldShouldStay, planVideoSync, VIDEO_RING_LENGTH, type VideoStageStatus } from "../../components/VideoStage";

export const videoStageChecks = {
  component: Boolean(VideoStage),
  fallbackStatuses: ["not-checked", "unavailable", "error", "fallback"] satisfies VideoStageStatus[],
  lifecycleStatuses: (["loading", "ready", "playing", "paused", "ended"] as VideoStageStatus[]).map((status) => status satisfies VideoStageStatus),
  usesHtmlVideo: true,
  fullscreenLabel: "Enter fullscreen",
  clockLifecycle: ["onResize", "onSeek", "onPause", "onResume", "onEnded"],
};

if (!videoStageChecks.component || !videoStageChecks.usesHtmlVideo) throw new Error("Video stage contract failed");

describe("Video stage download-progress ring", () => {
  it("renders an indeterminate green spinner while no percentage is known", () => {
    const html = renderToStaticMarkup(<VideoStage status="loading" />);
    expect(html).toContain("luno-video-loading");
    expect(html).toContain("luno-video-progress"); // ring badge present
    // Exact class match: the chrome stylesheet legitimately mentions
    // .is-determinate inside its :not() selector for the spin animation.
    expect(html).toContain('<span class="luno-video-progress" style=');
    expect(html).not.toContain('class="luno-video-progress is-determinate"');
    expect(html).toContain("Searching for video…");
  });

  it("keeps the spinner and explains the search phase at 0%", () => {
    // yt-dlp reports 0 until the video resolves; a determinate 0% ring looked
    // stuck, so 0 stays an indeterminate spinner with a search caption.
    const html = renderToStaticMarkup(<VideoStage status="loading" downloadProgress={0} />);
    expect(html).not.toContain('class="luno-video-progress is-determinate"');
    expect(html).toContain("Searching for video…");
  });

  it("renders a determinate ring that fills with the reported percentage", () => {
    const html = renderToStaticMarkup(<VideoStage status="loading" downloadProgress={42} />);
    expect(html).toContain("is-determinate");
    // Ring fill offset matches the 42 % geometry (0 = complete circle).
    expect(html).toContain(`stroke-dashoffset="${ringDashOffset(42)}"`);
    expect(html).toContain(">42%</b>");
    expect(html).toContain("Downloading video… 42%");
  });

  it("completes the circle exactly at 100 %", () => {
    expect(ringDashOffset(100)).toBe(0);
    const html = renderToStaticMarkup(<VideoStage status="loading" downloadProgress={100} />);
    expect(html).toContain('stroke-dashoffset="0"');
    expect(html).toContain(">100%</b>");
  });

  it("clamps out-of-range percentages instead of drawing impossible rings", () => {
    expect(ringDashOffset(-5)).toBe(ringDashOffset(0));
    expect(ringDashOffset(150)).toBe(ringDashOffset(100));
    expect(ringDashOffset(Number.NaN)).toBe(ringDashOffset(0));
    // Empty ring at 0 %: the full circumference as offset.
    expect(ringDashOffset(0)).toBeCloseTo(VIDEO_RING_LENGTH, 1);
  });
});

describe("Video stage end-of-media hold", () => {
  it("does not hold when the clip never ended", () => {
    expect(endedHoldShouldStay(null, true, 500_000)).toBe(false);
  });

  it("holds the final frame while the SONG keeps playing past the clip end", () => {
    // Clip ended at 120 s; audio now at 130 s → stay frozen, never restart.
    expect(endedHoldShouldStay(120_000, true, 130_000)).toBe(true);
  });

  it("releases only when the audio seeks meaningfully back BEFORE the end", () => {
    expect(endedHoldShouldStay(120_000, true, 60_000)).toBe(false);
    // Within the 1.5 s grace window around the end point → still held.
    expect(endedHoldShouldStay(120_000, true, 119_000)).toBe(true);
  });

  it("keeps holding across a song pause so resume cannot replay the clip", () => {
    expect(endedHoldShouldStay(120_000, false, 130_000)).toBe(true);
  });
});

describe("Video transport sync planner", () => {
  const base = { endedHoldAtMs: null as number | null, positionMs: 0 };

  it("pauses the element when the song pauses", () => {
    expect(planVideoSync({ ...base, playing: false, positionMs: 1000, elementPaused: false, elementSeconds: 1.2 }).action).toBe("pause");
  });

  it("FREEZES on sub-second staleness instead of rewinding the frame after a pause", () => {
    // Regression: the element freezes up to ~1 heartbeat (~400 ms) ahead of
    // the last published tick; chasing that delta visibly rewound the frame
    // right after every pause.
    expect(planVideoSync({ ...base, playing: false, positionMs: 60_000, elementPaused: true, elementSeconds: 60.35 }).action).toBe("none");
  });

  it("still follows a REAL navigation while paused (scrub)", () => {
    expect(planVideoSync({ ...base, playing: false, positionMs: 45_000, elementPaused: true, elementSeconds: 12 })).toEqual({ action: "seek", toMs: 45_000 });
  });

  it("resumes IN PLACE when the frozen frame is close to the song position", () => {
    // Regression: re-seeking every unpause made the picture jump back.
    expect(planVideoSync({ ...base, playing: true, positionMs: 90_500, elementPaused: true, elementSeconds: 90.8 })).toEqual({ action: "play", seekToMs: null });
  });

  it("seek-rejoins only for a genuine desync on resume", () => {
    expect(planVideoSync({ ...base, playing: true, positionMs: 120_000, elementPaused: true, elementSeconds: 90 })).toEqual({ action: "play", seekToMs: 120_000 });
  });

  it("leaves a freely-playing element alone while drift is within jitter", () => {
    expect(planVideoSync({ ...base, playing: true, positionMs: 30_400, elementPaused: false, elementSeconds: 30.7 }).action).toBe("none");
  });

  it("corrects real desyncs while playing", () => {
    expect(planVideoSync({ ...base, playing: true, positionMs: 42_000, elementPaused: false, elementSeconds: 40 })).toEqual({ action: "play", seekToMs: 42_000 });
  });

  it("lets an in-flight seek land instead of restarting it every tick", () => {
    // Regression: WKWebView restarts an async seek on every currentTime
    // assignment, so heartbeat ticks re-issuing the same scrub made big
    // jumps land late (the 'seeking' flag + pending target guard this).
    expect(planVideoSync({ ...base, playing: true, positionMs: 62_400, elementPaused: false, elementSeconds: 20, elementSeeking: true, pendingSeekMs: 62_500 }).action).toBe("none");
  });

  it("supersedes an in-flight seek when the user scrubs somewhere else", () => {
    expect(planVideoSync({ ...base, playing: true, positionMs: 180_000, elementPaused: false, elementSeconds: 20, elementSeeking: true, pendingSeekMs: 62_500 })).toEqual({ action: "play", seekToMs: 180_000 });
  });
});
