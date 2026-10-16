import { useSyncExternalStore } from "react";

export type AccentColor = "green" | "orange";
export const ACCENT_COLORS: ReadonlyArray<{ value: AccentColor; label: string; color: string }> = [
  { value: "green", label: "Green", color: "#1db954" },
  { value: "orange", label: "Orange", color: "rgb(250, 141, 118)" },
];

const STORAGE_KEY = "luno.appearance.accent";
let accent: AccentColor = "green";
const listeners = new Set<() => void>();

export function initializeAppearance() {
  try {
    accent = window.localStorage.getItem(STORAGE_KEY) === "orange" ? "orange" : "green";
  } catch {
    accent = "green";
  }
  document.documentElement.dataset.accent = accent;
}

export function setAccentColor(value: AccentColor) {
  accent = value;
  document.documentElement.dataset.accent = value;
  try {
    window.localStorage.setItem(STORAGE_KEY, value);
  } catch { /* Keep the current session usable when storage is unavailable. */ }
  listeners.forEach((listener) => listener());
}

const subscribe = (listener: () => void) => {
  listeners.add(listener);
  return () => { listeners.delete(listener); };
};

export function useAccentColor(): AccentColor {
  return useSyncExternalStore(subscribe, () => accent, () => "green");
}
