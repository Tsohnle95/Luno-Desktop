import { clampScrollTop, ROUTES, routeLabels } from "../app/Shell";

// Deterministic, browser-independent geometry checks for the screenshot harness.
export const shellVisualChecks = {
  routes: ROUTES.length === 7 && ROUTES.every((route) => typeof routeLabels[route] === "string") && routeLabels.library === "Home",
  minimums: { sidebar: 240, content: 400, queue: 240, player: 94, row: 56 },
  scrollClamps: clampScrollTop(-10, 1000, 400) === 0 && clampScrollTop(900, 1000, 400) === 600 && clampScrollTop(10, 100, 400) === 0,
};

if (!shellVisualChecks.routes || !shellVisualChecks.scrollClamps) throw new Error("Shell visual contract failed");
