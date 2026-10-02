import { afterEach, describe, expect, it, vi } from "vitest";
import { cachedTrackDuration, clearTrackDurationCacheForTests, requestTrackDuration } from "../state/track_durations";

describe("track duration cache", () => {
  afterEach(() => clearTrackDurationCacheForTests());

  it("coalesces duplicate probes and retains the result for a later route mount", async () => {
    const fetch = vi.fn(async () => 183_000);
    const first = requestTrackDuration("track-1", fetch);
    const second = requestTrackDuration("track-1", fetch);
    await expect(Promise.all([first, second])).resolves.toEqual([183_000, 183_000]);
    expect(fetch).toHaveBeenCalledTimes(1);
    expect(cachedTrackDuration("track-1")).toEqual({ found: true, value: 183_000 });
    await expect(requestTrackDuration("track-1", fetch)).resolves.toBe(183_000);
    expect(fetch).toHaveBeenCalledTimes(1);
  });

  it("retains a definitive no-duration result instead of reprobeing every scan", async () => {
    const fetch = vi.fn(async () => null);
    await expect(requestTrackDuration("track-without-duration", fetch)).resolves.toBeNull();
    expect(cachedTrackDuration("track-without-duration")).toEqual({ found: true, value: null });
    await expect(requestTrackDuration("track-without-duration", fetch)).resolves.toBeNull();
    expect(fetch).toHaveBeenCalledTimes(1);
  });
});
