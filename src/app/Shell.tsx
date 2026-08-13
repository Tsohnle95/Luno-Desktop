import { useState, useSyncExternalStore, type ReactNode } from "react";
import "../styles/tokens.css";
import "../styles/layout.css";
import { SettingsView } from "../views/SettingsView";
import { ErrorLogView } from "../views/ErrorLogView";
import { LibraryView } from "../views/LibraryView";
import { RecentView } from "../views/RecentView";
import { SongPageView } from "../views/SongPageView";
import { DownloaderView } from "../views/DownloaderView";
import { DiscoverView } from "../views/DiscoverView";
import type { BackendProjection } from "../state/backend_projection";

export const ROUTES = ["library", "recent", "discover", "downloader", "settings", "song_page", "error_log"] as const;
export type Route = (typeof ROUTES)[number];
export const routeLabels: Record<Route, string> = {
  library: "Home", recent: "Recent", discover: "Discover", downloader: "Downloader", settings: "Settings", song_page: "Song", error_log: "Error Log",
};

export interface ShellProps { views?: Partial<Record<Route, ReactNode>>; queue?: ReactNode; player?: ReactNode; backendProjection?: { getSnapshot: () => BackendProjection; subscribe: (listener: () => void) => () => void }; }

export function clampScrollTop(scrollTop: number, scrollHeight: number, clientHeight: number): number {
  return Math.min(Math.max(0, scrollTop), Math.max(0, scrollHeight - clientHeight));
}

export function Shell({ views = {}, queue, player, backendProjection }: ShellProps) {
  const [route, setRoute] = useState<Route>("library");
  const projection = useSyncExternalStore(backendProjection?.subscribe ?? (() => () => {}), backendProjection?.getSnapshot ?? (() => ({ library: null, playback: null } as BackendProjection)));
  const content = views[route] ?? (route === "library" ? <LibraryView /> : route === "recent" ? <RecentView /> : route === "discover" ? <DiscoverView /> : route === "downloader" ? <DownloaderView /> : route === "settings" ? <SettingsView onOpenErrorLog={() => setRoute("error_log")} /> : route === "error_log" ? <ErrorLogView /> : route === "song_page" ? <SongPageView /> : <div className="luno-route-empty">{routeLabels[route]}</div>);
  return <div className="luno-shell" data-route={route}>
    <aside className="luno-sidebar" data-slot="sidebar">
      <div className="luno-brand" aria-label="Luno">Luno</div>
      <nav className="luno-nav" aria-label="Primary navigation"><ul className="luno-nav-list">
        {ROUTES.map((item) => <li key={item}><button className="luno-nav-button" type="button" aria-current={route === item ? "page" : undefined} onClick={() => setRoute(item)}>{routeLabels[item]}</button></li>)}
      </ul></nav>
      <div className="luno-playlists luno-scroll" data-slot="playlists" aria-label="Playlists" />
    </aside>
    <main className="luno-main" data-slot="main"><div className="luno-main-content luno-scroll">{content}</div></main>
    <aside className="luno-queue luno-scroll" data-slot="queue" aria-label="Queue">{queue}</aside>
    <footer className="luno-player" data-slot="player" aria-label="Player">{player}</footer>
  </div>;
}
