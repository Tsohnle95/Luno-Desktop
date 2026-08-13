export type UiTab = "library" | "recent" | "discover" | "downloader" | "settings";

export interface UiState {
  activeTab: UiTab;
  selectedTrackId: string | null;
  selectedPlaylistId: string | null;
  dialog: string | null;
  scroll: Record<string, number>;
}

export const initialUiState = (): UiState => ({ activeTab: "library", selectedTrackId: null, selectedPlaylistId: null, dialog: null, scroll: {} });

export function createUiState() {
  let state = initialUiState();
  const listeners = new Set<() => void>();
  const update = (patch: Partial<UiState>) => { state = { ...state, ...patch }; listeners.forEach((listener) => listener()); };
  return {
    getSnapshot: () => state,
    subscribe: (listener: () => void) => { listeners.add(listener); return () => listeners.delete(listener); },
    setActiveTab: (activeTab: UiTab) => update({ activeTab }),
    setSelection: (selectedTrackId: string | null, selectedPlaylistId: string | null = state.selectedPlaylistId) => update({ selectedTrackId, selectedPlaylistId }),
    setDialog: (dialog: string | null) => update({ dialog }),
    setScroll: (key: string, position: number) => update({ scroll: { ...state.scroll, [key]: position } }),
  };
}
