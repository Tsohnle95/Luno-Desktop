import { describe, expect, it } from "vitest";
import { bumpPlayCount, madeForYouIds, popularTrackIds, recordDownloadCompletions } from "../state/play_stats";

describe("play stats", () => {
  it("counts plays per track without mutating the previous map", () => {
    const counts = bumpPlayCount({ t1: 2 }, "t1");
    expect(counts.t1).toBe(3);
    expect(counts.t2).toBeUndefined();
    const next = bumpPlayCount(counts, "t2");
    expect(next.t2).toBe(1);
    // Original untouched — React state updates rely on this.
    expect(counts.t2).toBeUndefined();
  });

  it("ranks popularity descending with alphabetical tie-breaks", () => {
    const counts = { b: 1, a: 3, c: 3, d: 0 };
    expect(popularTrackIds(counts, 10)).toEqual(["a", "c", "b"]);
    expect(popularTrackIds(counts, 2)).toEqual(["a", "c"]);
    expect(popularTrackIds({}, 5)).toEqual([]);
  });

  it("journals download completions newest-first, one entry per track", () => {
    const merged = recordDownloadCompletions([{ trackId: "t1", at: 10 }, { trackId: "t2", at: 5 }], [{ trackId: "t1", at: 99 }, { trackId: "t3", at: 100 }]);
    expect(merged.map((entry) => entry.trackId)).toEqual(["t3", "t1", "t2"]);
    expect(merged.find((entry) => entry.trackId === "t1")?.at).toBe(99);
  });

  it("derives a stable made-for-you mix from catalogue membership", () => {
    const ids = Array.from({ length: 120 }, (_, index) => `t${index}`);
    const first = madeForYouIds(ids, 50);
    expect(first).toHaveLength(50);
    // Same membership → identical mix across renders.
    expect(madeForYouIds(ids, 50)).toEqual(first);
    // Different membership → reshuffled.
    expect(madeForYouIds([...ids, "t-new"], 50)).not.toEqual(first);
    // Every entry comes from the catalogue and there are no duplicates.
    expect(new Set(first).size).toBe(50);
    expect(first.every((id) => ids.includes(id))).toBe(true);
    expect(madeForYouIds([], 50)).toEqual([]);
  });
});
