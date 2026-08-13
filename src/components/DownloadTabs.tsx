import type { ReactNode } from "react";

export const DOWNLOAD_TABS = ["search", "yt-csv", "direct-url", "sync", "url-queue", "duplicates", "allowed-dups"] as const;
export type DownloadTab = (typeof DOWNLOAD_TABS)[number];
const labels: Record<DownloadTab, string> = { search: "Search", "yt-csv": "YT / CSV", "direct-url": "Direct URL", sync: "Sync", "url-queue": "URL Queue", duplicates: "Duplicates", "allowed-dups": "Allowed Dups" };
export interface DownloadTabsProps { active: DownloadTab; onChange: (tab: DownloadTab) => void; children?: ReactNode }

export function DownloadTabs({ active, onChange, children }: DownloadTabsProps) {
  return <div className="download-tabs"><div role="tablist" aria-label="Download sources" style={{ display: "flex", gap: 4, overflowX: "auto", borderBottom: "1px solid var(--luno-border)" }}>{DOWNLOAD_TABS.map((tab) => <button key={tab} type="button" role="tab" aria-selected={active === tab} aria-controls={`download-panel-${tab}`} id={`download-tab-${tab}`} onClick={() => onChange(tab)} style={{ whiteSpace: "nowrap", border: 0, borderBottom: active === tab ? "3px solid var(--luno-green)" : "3px solid transparent", padding: "12px 10px", color: active === tab ? "var(--luno-text)" : "var(--luno-text-2)", background: "transparent", cursor: "pointer" }}>{labels[tab]}</button>)}</div><div role="tabpanel" id={`download-panel-${active}`} aria-labelledby={`download-tab-${active}`} tabIndex={0}>{children}</div></div>;
}
