import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    include: ["src/test/bridge.test.ts", "src/test/dialogs.test.tsx"],
  },
});
