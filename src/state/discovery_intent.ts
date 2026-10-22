export function requestDiscoveryIntent(missingCredentials: boolean, actions: { setRequested: (value: boolean) => void; reportError: (message: string) => void }): void {
  if (missingCredentials) {
    actions.reportError("Add a Last.fm API key in Settings to enable discovery mode.");
    return;
  }
  // Intent survives empty or pending results. Shell admits a projection only
  // when the audible physical seed has recommendations, preserving normal Next.
  actions.setRequested(true);
}

export function discoveryResultsReady(requested: boolean, seedKey: string | null, results: { seedKey: string | null; state: string; items: readonly unknown[] }): boolean {
  return requested && seedKey !== null && results.seedKey === seedKey
    && results.state === "ready" && results.items.length > 0;
}
