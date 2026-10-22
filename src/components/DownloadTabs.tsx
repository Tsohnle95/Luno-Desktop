import type { KeyboardEvent, ReactNode } from "react";

export const DOWNLOAD_TABS = ["search", "sync", "duplicates"] as const;
export type DownloadTab = (typeof DOWNLOAD_TABS)[number];
const labels: Record<DownloadTab, string> = { search: "Search", sync: "Sync", duplicates: "Duplicates" };
export interface DownloadTabsProps { active: DownloadTab; onChange: (tab: DownloadTab) => void; children?: ReactNode }

export function DownloadTabs({ active, onChange, children }: DownloadTabsProps) {
  const move = (event: KeyboardEvent, delta: number) => {
    const index = DOWNLOAD_TABS.indexOf(active);
    const next = DOWNLOAD_TABS[(index + delta + DOWNLOAD_TABS.length) % DOWNLOAD_TABS.length];
    onChange(next);
    document.getElementById(`download-tab-${next}`)?.focus();
  };
  return <div className="download-tabs"><div role="tablist" aria-label="Download sources" style={{ display: "flex", gap: 4, overflowX: "auto", borderBottom: "1px solid var(--luno-border)" }}>{DOWNLOAD_TABS.map((tab) => <button key={tab} type="button" role="tab" aria-selected={active === tab} aria-controls={`download-panel-${tab}`} id={`download-tab-${tab}`} tabIndex={active === tab ? 0 : -1} onKeyDown={(event) => { if (event.key === "ArrowRight") { event.preventDefault(); move(event, 1); } else if (event.key === "ArrowLeft") { event.preventDefault(); move(event, -1); } }} onClick={() => onChange(tab)} style={{ whiteSpace: "nowrap", border: 0, borderBottom: active === tab ? "3px solid var(--luno-accent)" : "3px solid transparent", padding: "12px 10px", color: active === tab ? "var(--luno-text)" : "var(--luno-text-2)", background: "transparent", cursor: "pointer" }}>{labels[tab]}</button>)}</div><div role="tabpanel" id={`download-panel-${active}`} aria-labelledby={`download-tab-${active}`} tabIndex={0}>{children}</div></div>;
}
