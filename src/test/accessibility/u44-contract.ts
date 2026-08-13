/** Browser-runner contract: the target must satisfy these semantic checks. */
export const U44_ACCESSIBILITY_CHECKS = [
  "primary navigation has an accessible name and exactly one aria-current=page",
  "queue and player landmarks have accessible names",
  "all buttons have an accessible name",
  "keyboard Tab order reaches navigation, main content, queue, and player",
  "focus remains visible and route changes move focus to the route heading",
  "dialogs trap focus and Escape closes without losing the invoking control",
] as const;

export const U44_ACCESSIBILITY_RECEIPT = {
  status: "inconclusive",
  reason: "Requires a browser/Playwright runner; source inspection is not an accessibility pass.",
};
