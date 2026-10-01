import { useState, type ChangeEvent, type ReactNode } from "react";

/** Destinations reachable from the settings rail. */
export type SettingsNavLink = "home" | "artwork" | "sync_review";

export interface SettingsViewProps {
  lastfmKey?: string;
  keychainStatus?: "available" | "missing" | "error" | "loading";
  /** Persists the Last.fm API key (stored privately in Luno's app data). */
  onSaveLastfmKey?: (key: string) => void;
  musicRoot?: string;
  libraryRootPath?: string;
  onLibraryRootSubmit?: (path: string) => void;
  autoDiscovery?: boolean;
  loading?: boolean;
  error?: string | null;
  onLastfmKeyChange?: (value: string) => void;
  onChooseMusicRoot?: () => void;
  onAutoDiscoveryChange?: (value: boolean) => void;
  transferManifest?: string;
  transferDestination?: string;
  onTransferManifestChange?: (value: string) => void;
  onTransferDestinationChange?: (value: string) => void;
  onTransferPreview?: () => void;
  onTransferApply?: () => void;
  onTransferExport?: () => void;
  transferResult?: string;
  /** Rail shortcuts into the app sections (only rendered when provided). */
  onNavigate?: (target: SettingsNavLink) => void;
  /** When set, the rail tab's screen renders in place of the settings panels. */
  screen?: ReactNode;
  activeScreen?: SettingsNavLink | null;
  /** Fired when the user picks a settings category (closes any open screen). */
  onScreenClosed?: () => void;
}

const CATEGORIES = ["Library", "Discovery", "Transfer"] as const;
type Category = (typeof CATEGORIES)[number];

const inputStyle = { width: "100%", padding: "11px 12px", color: "var(--luno-text)", background: "var(--luno-bg-3)", border: "1px solid var(--luno-border)", borderRadius: "var(--luno-radius)" };
const buttonStyle = { border: 0, borderRadius: "var(--luno-radius)", padding: "10px 14px", color: "var(--luno-bg)", background: "var(--luno-green)", fontWeight: 700, cursor: "pointer" };
const buttonsDisabledStyle = (disabled: boolean) => ({ ...buttonStyle, opacity: disabled ? 0.5 : 1, cursor: disabled ? "not-allowed" : "pointer" });

/**
 * Dedicated two-pane settings page: a sticky category rail on the left and
 * the matching panel content on the right. The profile avatar in the top-left
 * of the shell navigates here.
 */
export function SettingsView({
  lastfmKey = "", keychainStatus = "available", musicRoot = "Not configured", autoDiscovery = false,
  loading = false, error = null, onLastfmKeyChange, onSaveLastfmKey,
  libraryRootPath, onLibraryRootSubmit, onChooseMusicRoot, onAutoDiscoveryChange,
  transferManifest = "", transferDestination = "", transferResult = "", onTransferManifestChange, onTransferDestinationChange, onTransferPreview, onTransferApply, onTransferExport,
  onNavigate, screen = null, activeScreen = null, onScreenClosed,
}: SettingsViewProps) {
  const [category, setCategory] = useState<Category>("Library");
  const [key, setKey] = useState(lastfmKey);
  const [keyError, setKeyError] = useState<string | null>(null);
  const [keySaved, setKeySaved] = useState(false);
  const [rootInput, setRootInput] = useState(libraryRootPath ?? "");
  const [rootError, setRootError] = useState<string | null>(null);
  const updateRoot = (value: string) => {
    setRootInput(value);
    setRootError(value.trim().length > 0 && !value.trim().startsWith("/") ? "Enter an absolute folder path." : null);
  };
  const updateKey = (event: ChangeEvent<HTMLInputElement>) => {
    const value = event.target.value;
    setKey(value);
    setKeyError(value.trim() && value.trim().length < 8 ? "Enter a valid Last.fm API key." : null);
    onLastfmKeyChange?.(value);
  };
  const submitKey = () => {
    if (!key.trim() || keyError) return;
    onSaveLastfmKey?.(key.trim());
    setKeySaved(true);
    setKey("");
  };
  const section = (title: string, children: ReactNode) => <section aria-labelledby={`settings-${title}`} style={{ marginBottom: 34 }}>
    <h2 id={`settings-${title}`} style={{ fontSize: 16, margin: "0 0 14px", borderBottom: "1px solid var(--luno-border)", paddingBottom: 10 }}>{title}</h2>{children}
  </section>;
  const panels: Record<Category, ReactNode> = {
    Library: section("Storage & Library", onLibraryRootSubmit ? <div><label htmlFor="music-root">Music folder</label><div style={{ display: "flex", gap: 10, marginTop: 8 }}><input id="music-root" autoCorrect="off" spellCheck={false} value={rootInput} onChange={(e) => updateRoot(e.target.value)} placeholder="/Users/you/Music" aria-describedby="music-root-error" style={{ ...inputStyle, flex: 1 }} /><button type="button" disabled={rootInput.trim().length === 0 || rootError !== null} onClick={() => onLibraryRootSubmit(rootInput)} style={buttonsDisabledStyle(rootInput.trim().length === 0 || rootError !== null)}>Set folder</button></div>{rootError && <p id="music-root-error" role="alert" style={{ color: "var(--luno-red)", margin: "6px 0" }}>{rootError}</p>}<p style={{ color: "var(--luno-text-2)" }}>Enter an absolute folder path; Luno scans it as your music library and downloads land inside it.</p></div> : <div><label htmlFor="music-root">Music folder</label><div style={{ display: "flex", gap: 10, marginTop: 8 }}><input id="music-root" autoCorrect="off" spellCheck={false} readOnly value={musicRoot} style={{ ...inputStyle, flex: 1 }} /><button type="button" onClick={onChooseMusicRoot} style={buttonStyle}>Change</button></div><p style={{ color: "var(--luno-text-2)" }}>Library scanning and downloads use this folder.</p></div>),
    Discovery: section("Discovery & Personalization", <>
      <div style={{ background: "var(--luno-bg)", padding: 18, borderRadius: 8, marginBottom: 18 }}><strong>Connect to Last.fm</strong><p style={{ color: "var(--luno-text-2)", lineHeight: 1.5, marginBottom: 0 }}>Create an API key at Last.fm, then enter it below. The key is stored privately in Luno's app data.</p></div>
      <label htmlFor="lastfm-key">Last.fm API key</label><div style={{ display: "flex", gap: 10, marginTop: 8 }}><input id="lastfm-key" type="password" autoComplete="off" autoCorrect="off" spellCheck={false} value={key} onChange={updateKey} aria-describedby="lastfm-status lastfm-error" style={{ ...inputStyle, flex: 1 }} /><button type="button" disabled={!onSaveLastfmKey || !key.trim() || keyError !== null} onClick={submitKey} style={buttonsDisabledStyle(!key.trim() || keyError !== null)}>Save key</button></div>
      {keyError && <p id="lastfm-error" role="alert" style={{ color: "var(--luno-red)", margin: "6px 0" }}>{keyError}</p>}
      {keySaved && keychainStatus === "available" && <p role="status" style={{ color: "var(--luno-green)", margin: "6px 0 0" }}>API key saved — discovery is live.</p>}
      <p id="lastfm-status" role="status" style={{ color: keychainStatus === "error" ? "var(--luno-red)" : "var(--luno-text-2)", marginTop: 8 }}>{keychainStatus === "loading" ? "Checking Keychain..." : keychainStatus === "available" ? "An API key is configured; recommendations and Discover mode are enabled." : keychainStatus === "missing" ? "No API key configured yet — paste your Last.fm key above." : "Keychain unavailable. Your key was not saved."}</p>
      <label style={{ display: "flex", gap: 10, alignItems: "center", marginTop: 20 }}><input type="checkbox" checked={autoDiscovery} onChange={(e) => onAutoDiscoveryChange?.(e.target.checked)} /> Automatic recommendations</label>
    </>),
    Transfer: section("Library Transfer", <>
      <label htmlFor="transfer-manifest">Manifest JSON</label>
      <textarea id="transfer-manifest" autoCorrect="off" spellCheck={false} value={transferManifest} onChange={(e) => onTransferManifestChange?.(e.target.value)} rows={5} style={{ ...inputStyle, marginTop: 8, fontFamily: "monospace" }} />
      <label htmlFor="transfer-destination" style={{ display: "block", marginTop: 12 }}>Destination folder</label>
      <input id="transfer-destination" autoCorrect="off" spellCheck={false} value={transferDestination} onChange={(e) => onTransferDestinationChange?.(e.target.value)} style={{ ...inputStyle, marginTop: 8 }} />
        <div style={{ display: "flex", gap: 8, marginTop: 12 }}><button type="button" onClick={onTransferExport} style={buttonStyle}>Export current library</button><button type="button" onClick={onTransferPreview} style={buttonStyle}>Preview import</button><button type="button" onClick={onTransferApply} style={{ ...buttonStyle, background: "var(--luno-bg-3)", color: "var(--luno-text)" }}>Apply approved import</button></div>{transferResult && <p role="status" style={{ marginTop: 12 }}>{transferResult}</p>}
    </>),
  };
  return <div className="luno-settings">
    <nav className="luno-settings-nav" aria-label="Settings">
      <h1>Settings</h1>
      <button type="button" aria-current={activeScreen === "home" ? "true" : undefined} onClick={() => onNavigate?.("home")}>Home</button>
      <button type="button" aria-current={activeScreen === "artwork" ? "true" : undefined} onClick={() => onNavigate?.("artwork")}>Artwork</button>
      <button type="button" aria-current={activeScreen === "sync_review" ? "true" : undefined} onClick={() => onNavigate?.("sync_review")}>Sync Review</button>
      {CATEGORIES.map((item) => <button key={item} type="button" aria-current={activeScreen == null && category === item ? "true" : undefined} onClick={() => { setCategory(item); onScreenClosed?.(); }}>{item}</button>)}
    </nav>
    <div className="luno-settings-content">
      {screen ? <div className="luno-settings-screen">{screen}</div>
        : <>
          <p style={{ color: "var(--luno-text-2)", margin: "0 0 18px" }}>Preferences and account connections</p>
          {loading && <p role="status" aria-live="polite" style={{ color: "var(--luno-text-2)" }}>Loading settings...</p>}
          {error && <p role="alert" style={{ color: "var(--luno-red)", background: "var(--luno-bg-3)", padding: 12, borderRadius: 6 }}>{error}</p>}
          {/* Every panel stays mounted (hidden) so deep links and tests see the
              full settings surface while only the active category is visible. */}
          {(Object.keys(panels) as Category[]).map((item) => <div key={item} hidden={item !== category}>{panels[item]}</div>)}
        </>}
    </div>
  </div>;
}
