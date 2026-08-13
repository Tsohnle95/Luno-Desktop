/** Strict geometry and perceptual-color policy for screenshot runners. */
export const U44_GEOMETRY = {
  panes: { sidebarMin: 240, contentMin: 400, queueMin: 240 },
  playerHeight: 94,
  libraryRowHeight: 56,
  colorDeltaEThreshold: 3,
  geometryTolerancePx: 0,
  requiredCaptureWidths: [960, 1180, 1600],
} as const;

export function geometryReceipt(actual: Record<string, number>) {
  const expected = U44_GEOMETRY.panes;
  return {
    status: actual.sidebarMin === expected.sidebarMin && actual.contentMin === expected.contentMin && actual.queueMin === expected.queueMin && actual.playerHeight === U44_GEOMETRY.playerHeight && actual.libraryRowHeight === U44_GEOMETRY.libraryRowHeight ? "pass" : "fail",
    detail: "Geometry is exact; color comparison may use perceptual tolerance only.",
  } as const;
}
