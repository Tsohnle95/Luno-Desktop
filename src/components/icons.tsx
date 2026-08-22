import type { ReactNode, SVGProps } from "react";

type IconProps = SVGProps<SVGSVGElement> & { size?: number };

function Svg({ size = 16, children, ...rest }: IconProps & { children: ReactNode }) {
  return <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round" aria-hidden="true" focusable="false" {...rest}>{children}</svg>;
}

function Filled({ size = 16, children, ...rest }: IconProps & { children: ReactNode }) {
  return <svg width={size} height={size} viewBox="0 0 24 24" fill="currentColor" stroke="none" aria-hidden="true" focusable="false" {...rest}>{children}</svg>;
}

export const IconHome = (p: IconProps) => <Svg {...p}><path d="M3 9.5 12 3l9 6.5V20a1 1 0 0 1-1 1h-5v-7H10v7H4a1 1 0 0 1-1-1z" /></Svg>;
export const IconClock = (p: IconProps) => <Svg {...p}><circle cx="12" cy="12" r="9" /><polyline points="12 7 12 12 15.5 14" /></Svg>;
export const IconCompass = (p: IconProps) => <Svg {...p}><circle cx="12" cy="12" r="9" /><polygon points="16 8 13.8 13.8 8 16 10.2 10.2 16 8" /></Svg>;
export const IconDownload = (p: IconProps) => <Svg {...p}><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" /><polyline points="7 10 12 15 17 10" /><line x1="12" y1="15" x2="12" y2="3" /></Svg>;
export const IconSliders = (p: IconProps) => <Svg {...p}><line x1="4" y1="21" x2="4" y2="14" /><line x1="4" y1="10" x2="4" y2="3" /><line x1="12" y1="21" x2="12" y2="12" /><line x1="12" y1="8" x2="12" y2="3" /><line x1="20" y1="21" x2="20" y2="16" /><line x1="20" y1="12" x2="20" y2="3" /><line x1="1" y1="14" x2="7" y2="14" /><line x1="9" y1="8" x2="15" y2="8" /><line x1="17" y1="16" x2="23" y2="16" /></Svg>;
export const IconAlert = (p: IconProps) => <Svg {...p}><circle cx="12" cy="12" r="9" /><line x1="12" y1="8" x2="12" y2="12" /><line x1="12" y1="16" x2="12.01" y2="16" /></Svg>;
export const IconMusicNote = (p: IconProps) => <Svg {...p}><path d="M9 18V5l12-2v13" /><circle cx="6" cy="18" r="3" /><circle cx="18" cy="16" r="3" /></Svg>;
export const IconSearch = (p: IconProps) => <Svg {...p}><circle cx="11" cy="11" r="8" /><line x1="21" y1="21" x2="16.65" y2="16.65" /></Svg>;
export const IconChevronLeft = (p: IconProps) => <Svg {...p}><polyline points="15 18 9 12 15 6" /></Svg>;
export const IconChevronRight = (p: IconProps) => <Svg {...p}><polyline points="9 18 15 12 9 6" /></Svg>;
export const IconPlay = (p: IconProps) => <Filled {...p}><path d="M7 4.5a.8.8 0 0 1 1.22-.68l11 6.86a.8.8 0 0 1 0 1.36l-11 6.86A.8.8 0 0 1 7 19.5z" /></Filled>;
export const IconPause = (p: IconProps) => <Filled {...p}><rect x="6" y="4" width="4.4" height="16" rx="1" /><rect x="13.6" y="4" width="4.4" height="16" rx="1" /></Filled>;
export const IconSkipBack = (p: IconProps) => <Filled {...p}><path d="M19.5 5.3v13.4a.8.8 0 0 1-1.26.65L9 13.06v5.64a1 1 0 0 1-2 0V5.3a1 1 0 0 1 2 0v5.64l9.24-6.29a.8.8 0 0 1 1.26.65z" /></Filled>;
export const IconSkipForward = (p: IconProps) => <Filled {...p}><path d="M4.5 5.3v13.4a.8.8 0 0 0 1.26.65L15 13.06v5.64a1 1 0 0 0 2 0V5.3a1 1 0 0 0-2 0v5.64L5.76 4.65A.8.8 0 0 0 4.5 5.3z" /></Filled>;
export const IconShuffle = (p: IconProps) => <Svg {...p}><polyline points="16 3 21 3 21 8" /><line x1="4" y1="20" x2="21" y2="3" /><polyline points="21 16 21 21 16 21" /><line x1="15" y1="15" x2="21" y2="21" /><line x1="4" y1="4" x2="9" y2="9" /></Svg>;
export const IconRepeat = (p: IconProps) => <Svg {...p}><polyline points="17 1 21 5 17 9" /><path d="M3 11V9a4 4 0 0 1 4-4h14" /><polyline points="7 23 3 19 7 15" /><path d="M21 13v2a4 4 0 0 1-4 4H3" /></Svg>;
export const IconQueueList = (p: IconProps) => <Svg {...p}><line x1="9" y1="6" x2="21" y2="6" /><line x1="9" y1="12" x2="21" y2="12" /><line x1="9" y1="18" x2="21" y2="18" /><circle cx="4.5" cy="6" r="0.5" fill="currentColor" /><circle cx="4.5" cy="12" r="0.5" fill="currentColor" /><circle cx="4.5" cy="18" r="0.5" fill="currentColor" /></Svg>;
export const IconVolume = (p: IconProps) => <Svg {...p}><polygon points="11 5 6 9 3 9 3 15 6 15 11 19 11 5" fill="currentColor" stroke="none" /><path d="M15.54 8.46a5 5 0 0 1 0 7.07" /></Svg>;
export const IconClose = (p: IconProps) => <Svg {...p}><line x1="18" y1="6" x2="6" y2="18" /><line x1="6" y1="6" x2="18" y2="18" /></Svg>;
export const IconExpand = (p: IconProps) => <Svg {...p}><polyline points="15 3 21 3 21 9" /><polyline points="9 21 3 21 3 15" /><line x1="21" y1="3" x2="14" y2="10" /><line x1="3" y1="21" x2="10" y2="14" /></Svg>;
export const IconRefresh = (p: IconProps) => <Svg {...p}><polyline points="23 4 23 10 17 10" /><path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10" /></Svg>;
export const IconHeart = (p: IconProps) => <Svg {...p}><path d="M20.84 4.61a5.5 5.5 0 0 0-7.78 0L12 5.67l-1.06-1.06a5.5 5.5 0 0 0-7.78 7.78l1.06 1.06L12 21.23l7.78-7.78 1.06-1.06a5.5 5.5 0 0 0 0-7.78z" /></Svg>;
export const IconLibrary = (p: IconProps) => <Svg {...p}><path d="M2 3h6a4 4 0 0 1 4 4v14a3 3 0 0 0-3-3H2z" /><path d="M22 3h-6a4 4 0 0 0-4 4v14a3 3 0 0 1 3-3h7z" /></Svg>;
export const IconUser = (p: IconProps) => <Svg {...p}><circle cx="12" cy="8" r="4" /><path d="M4.5 21c.7-3.6 3.9-5.5 7.5-5.5s6.8 1.9 7.5 5.5" /></Svg>;
export const IconExternal = (p: IconProps) => <Svg {...p}><polyline points="9 18 15 12 9 6" /></Svg>;
