import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    include: ["src/test/bridge.test.ts", "src/test/dialogs.test.tsx", "src/test/gate2_smoke.test.ts", "src/test/gate3_smoke.test.ts", "src/test/gate4_smoke.test.ts", "src/test/gate5_smoke.test.ts", "src/test/views/downloader.test.tsx", "src/test/views/shell.test.tsx"],
  },
});
