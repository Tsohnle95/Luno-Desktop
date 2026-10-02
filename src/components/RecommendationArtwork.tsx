import { useState } from "react";

export const RECOMMENDATION_ART_DESIGN_COUNT = 75;

type Palette = { start: string; end: string; accent: string; secondary: string };

const palettes: Palette[] = [
  { start: "#25104A", end: "#0A6E8A", accent: "#FFB86B", secondary: "#B8F2E6" },
  { start: "#401515", end: "#B33B3B", accent: "#FFD166", secondary: "#FF8FA3" },
  { start: "#102A43", end: "#1D7874", accent: "#E8F1F2", secondary: "#F4A261" },
  { start: "#20113D", end: "#6A2C70", accent: "#FDE74C", secondary: "#9BC53D" },
  { start: "#042F2E", end: "#0F766E", accent: "#99F6E4", secondary: "#FBBF24" },
  { start: "#2E1065", end: "#BE185D", accent: "#F9A8D4", secondary: "#BFDBFE" },
  { start: "#172554", end: "#1E40AF", accent: "#93C5FD", secondary: "#FDE68A" },
  { start: "#3B1D0B", end: "#B45309", accent: "#FDE68A", secondary: "#FCA5A5" },
  { start: "#082F49", end: "#0369A1", accent: "#BAE6FD", secondary: "#A7F3D0" },
  { start: "#312E81", end: "#7C3AED", accent: "#E9D5FF", secondary: "#FDE047" },
  { start: "#3F0D12", end: "#8F2D56", accent: "#FFC857", secondary: "#B8F2E6" },
  { start: "#3B0A45", end: "#9D174D", accent: "#F0ABFC", secondary: "#FDE68A" },
  { start: "#052E16", end: "#166534", accent: "#BBF7D0", secondary: "#FDE68A" },
  { start: "#450A0A", end: "#BE123C", accent: "#FDA4AF", secondary: "#FDE68A" },
  { start: "#0C4A6E", end: "#155E75", accent: "#67E8F9", secondary: "#F0ABFC" },
  { start: "#1E1B4B", end: "#3730A3", accent: "#C4B5FD", secondary: "#F9A8D4" },
  { start: "#422006", end: "#92400E", accent: "#FDE68A", secondary: "#FCA5A5" },
  { start: "#134E4A", end: "#115E59", accent: "#5EEAD4", secondary: "#FCD34D" },
  { start: "#0E3B43", end: "#0F766E", accent: "#67E8F9", secondary: "#FDBA74" },
  { start: "#172554", end: "#0369A1", accent: "#BAE6FD", secondary: "#F0FDFA" },
  { start: "#581C87", end: "#9333EA", accent: "#F5D0FE", secondary: "#FDE68A" },
  { start: "#431407", end: "#EA580C", accent: "#FED7AA", secondary: "#BFDBFE" },
  { start: "#042F2E", end: "#047857", accent: "#A7F3D0", secondary: "#FDE68A" },
  { start: "#172554", end: "#4338CA", accent: "#C7D2FE", secondary: "#FECACA" },
  { start: "#3F0D12", end: "#BE123C", accent: "#FBCFE8", secondary: "#BFDBFE" },
  { start: "#082F49", end: "#164E63", accent: "#67E8F9", secondary: "#FDE68A" },
  { start: "#3B0764", end: "#86198F", accent: "#F0ABFC", secondary: "#A7F3D0" },
  { start: "#052E16", end: "#15803D", accent: "#86EFAC", secondary: "#FDE047" },
  { start: "#451A03", end: "#B45309", accent: "#FDBA74", secondary: "#BFDBFE" },
  { start: "#172554", end: "#1D4ED8", accent: "#93C5FD", secondary: "#F9A8D4" },
  { start: "#4A044E", end: "#BE185D", accent: "#F9A8D4", secondary: "#FDE68A" },
  { start: "#042F2E", end: "#0D9488", accent: "#99F6E4", secondary: "#FCA5A5" },
  { start: "#3B0764", end: "#A21CAF", accent: "#F5D0FE", secondary: "#86EFAC" },
  { start: "#3F0D12", end: "#9F1239", accent: "#FDA4AF", secondary: "#BAE6FD" },
  { start: "#0C4A6E", end: "#0284C7", accent: "#BAE6FD", secondary: "#F5D0FE" },
  { start: "#312E81", end: "#4F46E5", accent: "#C4B5FD", secondary: "#FDE68A" },
  { start: "#431407", end: "#C2410C", accent: "#FED7AA", secondary: "#A7F3D0" },
  { start: "#064E3B", end: "#047857", accent: "#6EE7B7", secondary: "#FDE68A" },
  { start: "#083344", end: "#155E75", accent: "#67E8F9", secondary: "#F0ABFC" },
  { start: "#1E1B4B", end: "#6D28D9", accent: "#DDD6FE", secondary: "#FBCFE8" },
  { start: "#500724", end: "#BE185D", accent: "#FBCFE8", secondary: "#BFDBFE" },
  { start: "#083344", end: "#0E7490", accent: "#67E8F9", secondary: "#FDE68A" },
  { start: "#365314", end: "#65A30D", accent: "#D9F99D", secondary: "#FDBA74" },
  { start: "#422006", end: "#D97706", accent: "#FDE68A", secondary: "#BFDBFE" },
  { start: "#1E1B4B", end: "#6D28D9", accent: "#A7F3D0", secondary: "#FDA4AF" },
  { start: "#581C87", end: "#A21CAF", accent: "#F5D0FE", secondary: "#99F6E4" },
  { start: "#052E16", end: "#166534", accent: "#BBF7D0", secondary: "#FCA5A5" },
  { start: "#450A0A", end: "#991B1B", accent: "#FECACA", secondary: "#FDE68A" },
  { start: "#164E63", end: "#0369A1", accent: "#BAE6FD", secondary: "#F5D0FE" },
  { start: "#164E63", end: "#0E7490", accent: "#BAE6FD", secondary: "#F9A8D4" },
  { start: "#1E3A8A", end: "#2563EB", accent: "#BFDBFE", secondary: "#FDE68A" },
  { start: "#701A75", end: "#C026D3", accent: "#F5D0FE", secondary: "#FDE68A" },
  { start: "#3F6212", end: "#4D7C0F", accent: "#BEF264", secondary: "#FDBA74" },
  { start: "#7C2D12", end: "#EA580C", accent: "#FED7AA", secondary: "#BAE6FD" },
  { start: "#042F2E", end: "#0F766E", accent: "#99F6E4", secondary: "#FBCFE8" },
  { start: "#172554", end: "#3730A3", accent: "#C7D2FE", secondary: "#FDA4AF" },
  { start: "#4C0519", end: "#9D174D", accent: "#FBCFE8", secondary: "#FDE68A" },
  { start: "#022C22", end: "#047857", accent: "#A7F3D0", secondary: "#BFDBFE" },
  { start: "#451A03", end: "#C2410C", accent: "#FED7AA", secondary: "#A7F3D0" },
  { start: "#1E1B4B", end: "#4338CA", accent: "#C4B5FD", secondary: "#FDE68A" },
  { start: "#431407", end: "#9A3412", accent: "#FED7AA", secondary: "#A7F3D0" },
  { start: "#082F49", end: "#075985", accent: "#BAE6FD", secondary: "#FCA5A5" },
  { start: "#3B0764", end: "#7E22CE", accent: "#E9D5FF", secondary: "#99F6E4" },
  { start: "#14532D", end: "#16A34A", accent: "#BBF7D0", secondary: "#FDE68A" },
  { start: "#3F0D12", end: "#BE123C", accent: "#FDA4AF", secondary: "#FDBA74" },
  { start: "#0F172A", end: "#1E40AF", accent: "#BFDBFE", secondary: "#F0ABFC" },
  { start: "#581C87", end: "#86198F", accent: "#F5D0FE", secondary: "#FDE68A" },
  { start: "#164E63", end: "#0F766E", accent: "#99F6E4", secondary: "#FCA5A5" },
  { start: "#365314", end: "#4D7C0F", accent: "#D9F99D", secondary: "#BFDBFE" },
  { start: "#7C2D12", end: "#C2410C", accent: "#FED7AA", secondary: "#FDE68A" },
  { start: "#0F3D3E", end: "#0D9488", accent: "#99F6E4", secondary: "#FDE68A" },
  { start: "#500724", end: "#9D174D", accent: "#FBCFE8", secondary: "#A7F3D0" },
  { start: "#064E3B", end: "#0F766E", accent: "#99F6E4", secondary: "#FDE68A" },
  { start: "#450A0A", end: "#B91C1C", accent: "#FECACA", secondary: "#BFDBFE" },
  { start: "#172554", end: "#1D4ED8", accent: "#93C5FD", secondary: "#A7F3D0" },
];

function normalizePart(value: string): string {
  return value
    .normalize("NFKD")
    .replace(/\p{M}/gu, "")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, " ")
    .trim()
    .replace(/\s+/g, " ");
}

export function normalizedRecommendationKey(artist: string, title: string): string {
  return `${normalizePart(artist)}|${normalizePart(title)}`;
}

function javaHashCode(str: string): number {
  let h = 0;
  for (let i = 0; i < str.length; i++) h = (Math.imul(31, h) + str.charCodeAt(i)) | 0;
  return h;
}

export function recommendationArtworkDesignIndex(artist: string, title: string): number {
  const key = normalizedRecommendationKey(artist, title);
  const hash = javaHashCode(key);
  return ((hash % RECOMMENDATION_ART_DESIGN_COUNT) + RECOMMENDATION_ART_DESIGN_COUNT) % RECOMMENDATION_ART_DESIGN_COUNT;
}

const LASTFM_PLACEHOLDER = "2a96cbd8b46e442fc41c2b86b821562f";

function artworkHost(url: string): string {
  try {
    // Recommendation providers return absolute URLs. The base keeps this
    // helper harmless for a relative local artwork path.
    return new URL(url, "https://luno.invalid").hostname.toLowerCase();
  } catch {
    return "";
  }
}

export function isPlaceholderArtwork(url: string | null | undefined): boolean {
  if (!url || !url.trim()) return true;
  const lower = url.trim().toLowerCase();
  if (lower.includes(LASTFM_PLACEHOLDER)) return true;
  const host = artworkHost(lower);
  const isLastFmHost = host === "last.fm" || host.endsWith(".last.fm") || host === "lastfm.freetls.fastly.net";
  if (isLastFmHost && /(?:^|[\/_-])(default|noimage|no_image|placeholder)(?:[\/_\.-]|$)/.test(lower)) return true;
  return false;
}

export interface RecommendationArtworkProps {
  artist: string;
  title: string;
  size?: number | string;
  className?: string;
  style?: React.CSSProperties;
}

export function RecommendationArtwork({ artist, title, size = 48, className, style }: RecommendationArtworkProps) {
  const index = recommendationArtworkDesignIndex(artist, title);
  const palette = palettes[index % palettes.length];
  const kind = index % 6;
  const dim = typeof size === "number" ? `${size}px` : size;
  const overlay = renderOverlay(kind, index, palette);
  return (
    <span
      className={`luno-rec-art${className ? ` ${className}` : ""}`}
      style={{
        width: dim,
        height: dim,
        flex: `0 0 ${dim}`,
        display: "inline-grid",
        placeItems: "center",
        overflow: "hidden",
        borderRadius: 6,
        background: `linear-gradient(135deg, ${palette.start}, ${palette.end})`,
        position: "relative",
        ...style,
      }}
      aria-hidden="true"
      data-design={index}
    >
      {overlay}
    </span>
  );
}

function renderOverlay(kind: number, index: number, palette: Palette) {
  const accent = palette.accent;
  const secondary = palette.secondary;
  const common: React.CSSProperties = { position: "absolute", pointerEvents: "none" };
  switch (kind) {
    case 0:
      return (
        <>
          <span style={{ ...common, width: "56%", height: "56%", borderRadius: "50%", background: secondary, opacity: 0.85, left: "32%", top: "18%" }} />
          <span style={{ ...common, width: "18%", height: "18%", borderRadius: "50%", background: accent, opacity: 0.95, left: "18%", top: "62%" }} />
          <span style={{ ...common, width: "70%", height: "70%", borderRadius: "50%", border: `2px solid rgba(255,255,255,0.22)`, left: "15%", top: "15%", background: "transparent" }} />
        </>
      );
    case 1:
      return (
        <>
          <span style={{ ...common, width: "140%", height: "18%", background: secondary, opacity: 0.78, transform: "rotate(18deg)", left: "-20%", top: "32%" }} />
          <span style={{ ...common, width: "140%", height: "7%", background: accent, opacity: 0.9, transform: "rotate(18deg)", left: "-20%", top: "58%" }} />
        </>
      );
    case 2:
      return (
        <>
          <span style={{ ...common, width: "44%", height: "44%", borderRadius: "50%", background: secondary, opacity: 0.9, left: "28%", top: "28%" }} />
          <span style={{ ...common, width: "28%", height: "28%", borderRadius: "50%", background: palette.start, opacity: 0.85, left: "36%", top: "36%" }} />
          <span style={{ ...common, width: "14%", height: "14%", borderRadius: "50%", background: accent, left: "43%", top: "43%" }} />
        </>
      );
    case 3:
      return (
        <>
          <span style={{ ...common, width: "140%", height: "1px", background: "rgba(255,255,255,0.42)", left: "-20%", top: "38%", transform: "rotate(-35deg)" }} />
          <span style={{ ...common, width: "140%", height: "1px", background: "rgba(255,255,255,0.42)", left: "-20%", top: "52%", transform: "rotate(-35deg)" }} />
          <span style={{ ...common, width: "64%", height: "14%", background: accent, left: "18%", top: "42%", transform: "rotate(-35deg)", borderRadius: 2 }} />
        </>
      );
    case 4:
      return (
        <>
          <span style={{ ...common, width: "84%", height: "34%", border: `2px solid ${secondary}`, borderRadius: 999, left: "8%", top: "30%", opacity: 0.9 }} />
          <span style={{ ...common, width: "58%", height: "84%", border: `1px solid rgba(255,255,255,0.22)`, borderRadius: 999, left: "21%", top: "8%", opacity: 0.9 }} />
          <span style={{ ...common, width: "12%", height: "12%", borderRadius: "50%", background: accent, left: "62%", top: "28%" }} />
        </>
      );
    default:
      return (
        <>
          <span style={{ ...common, width: "48%", height: "64%", borderRadius: "48% 52% 42% 58%", background: accent, left: "8%", top: "18%", opacity: 0.9, transform: "rotate(-18deg)" }} />
          <span style={{ ...common, width: "48%", height: "54%", borderRadius: "40% 60% 55% 45%", background: secondary, right: "8%", bottom: "12%", opacity: 0.85, transform: "rotate(20deg)" }} />
          <span style={{ ...common, width: "18%", height: "18%", borderRadius: "50%", background: "rgba(255,255,255,0.5)", left: "41%", top: "42%" }} />
        </>
      );
  }
}

export interface RecommendationArtworkImageProps {
  artist: string;
  title: string;
  artworkUrl?: string | null;
  loading?: boolean;
  eager?: boolean;
  size?: number | string;
  alt?: string;
  className?: string;
}

export function RecommendationArtworkImage({ artist, title, artworkUrl, loading = false, eager = false, size = 48, alt, className }: RecommendationArtworkImageProps) {
  const [failedSrc, setFailedSrc] = useState<string | null>(null);
  const usable = artworkUrl && !isPlaceholderArtwork(artworkUrl) && artworkUrl !== failedSrc;
  const dim = typeof size === "number" ? `${size}px` : size;
  if (usable) {
    return (
      <span className={`luno-artwork${className ? ` ${className}` : ""}`} style={{ width: dim, height: dim, flex: `0 0 ${dim}` }}>
        <img
          src={artworkUrl!}
          alt={alt ?? `${title} artwork`}
          decoding="async"
          loading={eager ? "eager" : "lazy"}
          style={{ width: "100%", height: "100%", objectFit: "cover", display: "block" }}
          onError={() => setFailedSrc(artworkUrl!)}
        />
      </span>
    );
  }
  if (loading) {
    return <span className={`luno-artwork${className ? ` ${className}` : ""}`} style={{ width: dim, height: dim, flex: `0 0 ${dim}` }} aria-hidden="true"><span className="luno-artwork-loading" /></span>;
  }
  return <RecommendationArtwork artist={artist} title={title} size={size} className={className} />;
}
