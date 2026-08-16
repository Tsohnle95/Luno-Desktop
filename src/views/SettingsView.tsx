import { useState, type ChangeEvent, type ReactNode } from "react";

export type SleepTimer = "Off" | "5 mins" | "15 mins" | "30 mins" | "60 mins";

export interface SettingsViewProps {
  lastfmKey?: string;
  keychainStatus?: "available" | "missing" | "error" | "loading";
  musicRoot?: string;
  autoDiscovery?: boolean;
  sleepTimer?: SleepTimer;
  mediaKeys?: boolean;
  loading?: boolean;
  error?: string | null;
  onLastfmKeyChange?: (value: string) => void;
  onChooseMusicRoot?: () => void;
  onAutoDiscoveryChange?: (value: boolean) => void;
  onSleepTimerChange?: (value: SleepTimer) => void;
  onMediaKeysChange?: (value: boolean) => void;
  transferManifest?: string;
  transferDestination?: string;
  onTransferManifestChange?: (value: string) => void;
  onTransferDestinationChange?: (value: string) => void;
  onTransferPreview?: () => void;
  onTransferApply?: () => void;
  onTransferExport?: () => void;
  transferResult?: string;
  onOpenErrorLog?: () => void;
}

const inputStyle = { width: "100%", padding: "11px 12px", color: "var(--luno-text)", background: "var(--luno-bg-3)", border: "1px solid var(--luno-border)", borderRadius: "var(--luno-radius)" };
const buttonStyle = { border: 0, borderRadius: "var(--luno-radius)", padding: "10px 14px", color: "var(--luno-bg)", background: "var(--luno-green)", fontWeight: 700, cursor: "pointer" };

export function SettingsView({
  lastfmKey = "", keychainStatus = "available", musicRoot = "Not configured", autoDiscovery = false,
  sleepTimer = "Off", mediaKeys = true, loading = false, error = null, onLastfmKeyChange,
  onChooseMusicRoot, onAutoDiscoveryChange, onSleepTimerChange, onMediaKeysChange, onOpenErrorLog,
  transferManifest = "", transferDestination = "", transferResult = "", onTransferManifestChange, onTransferDestinationChange, onTransferPreview, onTransferApply, onTransferExport,
}: SettingsViewProps) {
  const [key, setKey] = useState(lastfmKey);
  const [keyError, setKeyError] = useState<string | null>(null);
  const updateKey = (event: ChangeEvent<HTMLInputElement>) => {
    const value = event.target.value;
    setKey(value);
    setKeyError(value.trim() && value.trim().length < 8 ? "Enter a valid Last.fm API key." : null);
    onLastfmKeyChange?.(value);
  };
  const section = (title: string, children: ReactNode) => <section aria-labelledby={`settings-${title}`} style={{ marginBottom: 34 }}>
    <h2 id={`settings-${title}`} style={{ fontSize: 16, margin: "0 0 14px", borderBottom: "1px solid var(--luno-border)", paddingBottom: 10 }}>{title}</h2>{children}
  </section>;
  return <div style={{ minHeight: "100%", padding: "36px clamp(20px, 5vw, 48px) 60px", maxWidth: 820 }}>
    <header style={{ marginBottom: 34 }}><p style={{ color: "var(--luno-text-2)", margin: 0 }}>Preferences and account connections</p><h1 style={{ fontSize: 36, margin: "8px 0 0" }}>Settings</h1></header>
    {loading && <p role="status" aria-live="polite" style={{ color: "var(--luno-text-2)" }}>Loading settings...</p>}
    {error && <p role="alert" style={{ color: "var(--luno-red)", background: "var(--luno-bg-3)", padding: 12, borderRadius: 6 }}>{error}</p>}
    {section("Discovery & Personalization", <>
      <div style={{ background: "var(--luno-bg)", padding: 18, borderRadius: 8, marginBottom: 18 }}><strong>Connect to Last.fm</strong><p style={{ color: "var(--luno-text-2)", lineHeight: 1.5, marginBottom: 0 }}>Create an API key at Last.fm, then enter it below. The key is stored in the macOS Keychain and is never written to settings files.</p></div>
      <label htmlFor="lastfm-key">Last.fm API key</label><input id="lastfm-key" type="password" autoComplete="off" value={key} onChange={updateKey} aria-describedby="lastfm-status lastfm-error" style={{ ...inputStyle, marginTop: 8 }} />
      {keyError && <p id="lastfm-error" role="alert" style={{ color: "var(--luno-red)", margin: "6px 0" }}>{keyError}</p>}
      <p id="lastfm-status" role="status" style={{ color: keychainStatus === "error" ? "var(--luno-red)" : "var(--luno-text-2)", marginTop: 8 }}>{keychainStatus === "loading" ? "Checking Keychain..." : keychainStatus === "available" ? "Keychain storage available" : keychainStatus === "missing" ? "No Keychain key configured" : "Keychain unavailable. Your key was not saved."}</p>
      <label style={{ display: "flex", gap: 10, alignItems: "center", marginTop: 20 }}><input type="checkbox" checked={autoDiscovery} onChange={(e) => onAutoDiscoveryChange?.(e.target.checked)} /> Automatic recommendations</label>
    </>)}
    {section("Storage & Library", <div><label htmlFor="music-root">Music folder</label><div style={{ display: "flex", gap: 10, marginTop: 8 }}><input id="music-root" readOnly value={musicRoot} style={{ ...inputStyle, flex: 1 }} /><button type="button" onClick={onChooseMusicRoot} style={buttonStyle}>Change</button></div><p style={{ color: "var(--luno-text-2)" }}>Library scanning and downloads use this folder.</p></div>)}
    {section("Playback Settings", <>
      <label htmlFor="sleep-timer">Sleep timer</label><select id="sleep-timer" value={sleepTimer} onChange={(e) => onSleepTimerChange?.(e.target.value as SleepTimer)} style={{ ...inputStyle, marginTop: 8 }} aria-label="Sleep timer">{["Off", "5 mins", "15 mins", "30 mins", "60 mins"].map(value => <option key={value}>{value}</option>)}</select>
      <label style={{ display: "flex", gap: 10, alignItems: "center", marginTop: 22 }}><input type="checkbox" checked={mediaKeys} onChange={(e) => onMediaKeysChange?.(e.target.checked)} /> Global media keys (macOS remote commands)</label>
    </>)}
    {section("Library Transfer", <>
      <label htmlFor="transfer-manifest">Manifest JSON</label>
      <textarea id="transfer-manifest" value={transferManifest} onChange={(e) => onTransferManifestChange?.(e.target.value)} rows={5} style={{ ...inputStyle, marginTop: 8, fontFamily: "monospace" }} />
      <label htmlFor="transfer-destination" style={{ display: "block", marginTop: 12 }}>Destination folder</label>
      <input id="transfer-destination" value={transferDestination} onChange={(e) => onTransferDestinationChange?.(e.target.value)} style={{ ...inputStyle, marginTop: 8 }} />
        <div style={{ display: "flex", gap: 8, marginTop: 12 }}><button type="button" onClick={onTransferExport} style={buttonStyle}>Export current library</button><button type="button" onClick={onTransferPreview} style={buttonStyle}>Preview import</button><button type="button" onClick={onTransferApply} style={{ ...buttonStyle, background: "var(--luno-bg-3)", color: "var(--luno-text)" }}>Apply approved import</button></div>{transferResult && <p role="status" style={{ marginTop: 12 }}>{transferResult}</p>}
    </>)}
    {section("Troubleshooting", <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", gap: 16 }}><span style={{ color: "var(--luno-text-2)" }}>View internal application logs for technical support.</span><button type="button" onClick={onOpenErrorLog} style={{ ...buttonStyle, background: "var(--luno-bg-3)", color: "var(--luno-text)" }}>Open logs</button></div>)}
  </div>;
}
