import { useState, type ReactNode } from "react";
import "../styles/tokens.css";
import "../styles/layout.css";

export const ROUTES = ["library", "recent", "discover", "downloader", "settings"] as const;
export type Route = (typeof ROUTES)[number];
export const routeLabels: Record<Route, string> = {
  library: "Home", recent: "Recent", discover: "Discover", downloader: "Downloader", settings: "Settings",
};

export interface ShellProps { views?: Partial<Record<Route, ReactNode>>; queue?: ReactNode; player?: ReactNode; }

export function clampScrollTop(scrollTop: number, scrollHeight: number, clientHeight: number): number {
  return Math.min(Math.max(0, scrollTop), Math.max(0, scrollHeight - clientHeight));
}

export function Shell({ views = {}, queue, player }: ShellProps) {
  const [route, setRoute] = useState<Route>("library");
  const content = views[route] ?? <div className="luno-route-empty">{routeLabels[route]}</div>;
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
