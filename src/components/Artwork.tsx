import { useState, type ReactNode } from "react";

export interface ArtworkProps {
  src?: string | null;
  alt?: string;
  size?: number | string;
  className?: string;
  fallback?: ReactNode;
}

/** Album art with a deliberately local fallback; failed artwork must not cause a list redraw. */
export function Artwork({ src, alt = "", size = 44, className, fallback }: ArtworkProps) {
  const [failedSrc, setFailedSrc] = useState<string | null>(null);
  const showImage = Boolean(src) && src !== failedSrc;
  const style = { width: size, height: size, flex: `0 0 ${typeof size === "number" ? `${size}px` : size}` };
  return <span className={`luno-artwork${className ? ` ${className}` : ""}`} style={style} aria-hidden={alt ? undefined : true}>
    {showImage ? <img src={src!} alt={alt} decoding="async" loading="lazy" onError={() => setFailedSrc(src!)} /> : (fallback ?? <span className="luno-artwork-fallback" aria-label={alt || "Artwork unavailable"}>♫</span>)}
  </span>;
}
