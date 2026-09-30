import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    include: ["src/test/bridge.test.ts", "src/test/dialogs.test.tsx", "src/test/download_naming.test.ts", "src/test/gate2_smoke.test.ts", "src/test/gate3_smoke.test.ts", "src/test/gate4_smoke.test.ts", "src/test/gate5_smoke.test.ts", "src/test/play_stats.test.ts", "src/test/artwork_eviction.test.ts", "src/test/recommendation_artwork.test.tsx", "src/test/views/downloader.test.tsx", "src/test/views/playlist_picker.test.tsx", "src/test/views/library.test.tsx", "src/test/views/settings.test.tsx", "src/test/views/shell.test.tsx", "src/test/views/song_page.test.tsx", "src/test/views/video_stage.test.tsx"],
  },
});
