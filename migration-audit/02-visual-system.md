# Visual System Audit

## Design Tokens And Shell
- Status: implemented
- Evidence:
  - `theme.py:3-20`
  - `mobile-app/app/src/main/java/com/luno/mobile/ui/theme/Color.kt:5-32`
  - `mobile-app/app/src/main/java/com/luno/mobile/ui/theme/Theme.kt:12-45`
  - `mobile-app/app/src/main/java/com/luno/mobile/ui/shell/MainShell.kt:1387-1423`
- User behavior: Both applications present a dark music-player interface with white/secondary text and green accent actions. Desktop defines `BG`/surface/sidebar/player colors, status colors, and playlist gradients. Android defines a separate Material3 dark scheme, navigation colors, card/overlay surfaces, and a black/green shell wash. Android's tappable `Luno` header opens the settings drawer and includes the small green bar mark.
- State and data flow: Desktop views import color constants from `theme.py`; playlist artwork selects a deterministic gradient from the playlist name (`theme.py:22-35`). Android composables receive `LunoTheme`, `MaterialTheme.typography`, and named `Color.kt`/`Dimens.kt` values; shell background rendering is a modifier-level visual layer (`MainShell.kt:1277-1348`). These are independent runtime token sources, not a shared module.
- Persistence and compatibility: No visual tokens are persisted and the library-transfer contract does not carry UI state. The migration must preserve appearance without adding token data to the manifest.
- Async, lifecycle, and failure behavior: Desktop gradients are synchronous canvas drawing; artwork arrives asynchronously through the controller cache and UI-thread callback (`music_player.py:251-278`). Android artwork color extraction runs off the UI thread and animates into the gradient (`ArtworkImage.kt:119-144`).
- UI states and visual requirements: Preserve the desktop palette (`#121212`, `#181818`, `#282828`, `#1DB954`, `#FFFFFF`, `#B3B3B3`) as the desktop reference. Treat Android's `#101010`/`#202020`/`#292929`/`#1ED760` values as Android implementation evidence and parity guidance, not exact desktop tokens.
- Migration implementation requirements: Create an explicit target token map with documented desktop source values and deliberate Android substitutions. Do not infer that Material3 defaults, Android XML colors, or proposed migration styles are desktop behavior. Preserve the green wordmark accent, dark surfaces, playlist-specific gradients, and contrast hierarchy.
- Verification requirements: Verify token values against source; exercise each shell route and settings drawer in a desktop build and Android debug build. Automated repository tests do not provide screenshot/golden-image coverage, so visual checks require manual device/window review.
- Confidence: high
- Open questions: Whether the target should use desktop's `#121212` or Android's `#101010` as the canonical migration background is a product decision, not resolved by code.

## Typography And Layout Geometry
- Status: implemented
- Evidence:
  - `views/library.py:61-107,108-162,175-207`
  - `views/recent.py:132-203`
  - `views/song_page.py:307-366,1180-1193`
  - `mobile-app/app/src/main/java/com/luno/mobile/ui/theme/Type.kt:9-83`
  - `mobile-app/app/src/main/java/com/luno/mobile/ui/theme/Dimens.kt:5-27`
- User behavior: Desktop uses Helvetica Neue/Helvetica for headings and controls, Menlo for log/config text, fixed row rhythm, and responsive title wrapping. The Library has a 220 px header with 150 px artwork, a flexible action row, search row, and virtual track canvas. Recent rows are 56 px high. Song Page scales title/artist typography at a width breakpoint. Android centralizes a SansSerif Material3 scale from 10 sp labels through 28 sp headlines and defines 48 dp minimum touch targets, 24 dp icons, 64 dp mini-player/navigation bars, 48/120/280 dp artwork, and 0.5/3 dp divider/progress sizes.
- State and data flow: Desktop geometry is established in each Tk view and recalculated from `<Configure>` callbacks; Android geometry is composable and uses shared `Dimens` values. Desktop track/recent rows derive hit-testing and scroll height from the 56 px contract (`views/library.py:227-239`; `views/recent.py:138-207`).
- Persistence and compatibility: Font and dimension choices are presentation-only. Track names remain source data; desktop truncation/wrapping must not mutate filenames or metadata.
- Async, lifecycle, and failure behavior: Desktop list redraws are virtualized to visible canvas rows and artwork callbacks update the view. Android lazy lists and bounded artwork decoding are the performance boundary; `ArtworkImage.kt:47-52,80-107` explicitly caps thumbnail decoding and avoids subcomposition churn.
- UI states and visual requirements: Keep desktop's 56 px virtual-list row height and width-aware text truncation. Keep Android's 48 dp touch target floor and shared type scale where porting behavior to touch UI. Song Page's title should remain compact enough to leave room for media/recommendations; Android's full player uses the 280 dp large-art baseline documented by `Dimens.kt`.
- Migration implementation requirements: Convert pixel geometry to responsive CSS/layout tokens while preserving observable spacing/order, row hit areas, truncation, and heading hierarchy. Treat desktop's many inline font declarations as evidence to normalize, not as evidence of a single existing desktop typography token.
- Verification requirements: Check narrow and wide desktop windows, long titles, empty/short lists, and Android narrow screens. Confirm no row overlap, clipped logo/title, or inaccessible controls. No committed screenshot tests were found; add target visual regression coverage separately if required.
- Confidence: high
- Open questions: Exact font matching for the target is unspecified; desktop relies on platform fonts and Android uses system SansSerif.

## Artwork Treatments And Assets
- Status: implemented
- Evidence:
  - `widgets.py:239-331`
  - `rec_artwork.py:1-120`
  - `mobile-app/app/src/main/java/com/luno/mobile/ui/components/ArtworkImage.kt:42-107`
  - `mobile-app/app/src/main/java/com/luno/mobile/ui/components/ArtworkCollage.kt:22-74`
  - `mobile-app/app/src/main/java/com/luno/mobile/ui/components/RecommendationArtwork.kt:32-54,120-209`
  - `mobile-app/app/src/main/res/drawable/ic_music_note.xml`
  - `mobile-app/app/src/test/java/com/luno/mobile/ui/components/RecommendationArtworkTest.kt:8-34`
  - `mobile-app/app/src/test/java/com/luno/mobile/ui/components/ArtworkCollageTest.kt:8-22`
- User behavior: Desktop playlist art uses a gradient/music-note fallback, one image for a single track, or a four-quadrant collage for four tracks. Android uses rounded square artwork, crop-filled valid images, gradient/music-note fallback, and a 2x2 collage that skips tracks without cached art. Recommendation artwork has deterministic abstract fallbacks; Android contains 75 code-generated designs and desktop's recommendation renderer is the corresponding source-side fallback.
- State and data flow: Desktop `PlaylistArt` requests bounded cached thumbnails and redraws only on size/art state changes (`widgets.py:252-289`). Android `ArtworkImage` loads cached `file://` art with a decode-size cap; recommendation wrappers use identity-based fallback designs for missing/loading/error states. Artwork assets are mostly generated/cached at runtime; Android's tracked drawable set contains launcher and navigation/playback vector icons, not album bitmap content.
- Persistence and compatibility: Desktop artwork is derived from local audio paths/cache; Android persists extracted artwork paths beside Room tracks and uses app-private artwork cache as documented by `mobile-app/project_brain.md:385-387`. No artwork is part of the shared export manifest.
- Async, lifecycle, and failure behavior: Both clients display a non-empty fallback while artwork is unavailable and update after async retrieval. Android dominant-color gradients animate over 600 ms (`ArtworkImage.kt:133-143`); desktop uses deterministic static gradient fallback and bounded cache callbacks.
- UI states and visual requirements: Preserve crop-without-distortion, square/collage composition, rounded treatment on Android, desktop gradient fallback, stable recommendation identity, and no blank/white native image failure state. Do not copy Android's 75-palette implementation into desktop without checking the existing desktop `rec_artwork.py` behavior.
- Migration implementation requirements: Define artwork states explicitly: valid embedded/remote image, loading, missing, invalid/error, and deterministic fallback. Keep thumbnail decode/cache limits and async UI-thread delivery. Map desktop playlist gradients and Android recommendation art as separate visual families unless a product decision unifies them.
- Verification requirements: Run deterministic artwork tests, inspect single-art and collage cases, and manually verify loading/error/missing states on both platforms. Confirm launcher/vector assets resolve in the Android APK and desktop packaging continues to resolve assets through `resource_path` (`utils.py:87-94`).
- Confidence: high
- Open questions: The repository has no screenshot/golden asset fixtures proving exact crop, radius, or color appearance at runtime.

## Responsive, Scroll, And Interaction States
- Status: implemented
- Evidence:
  - `utils.py:4-84`
  - `views/library.py:215-239`
  - `views/song_page.py:284-305,397-418`
  - `widgets.py:92-237,338-442`
  - `mobile-app/app/src/main/java/com/luno/mobile/ui/home/HomeScreen.kt:405-728`
  - `mobile-app/app/src/main/java/com/luno/mobile/ui/player/FullPlayerScreen.kt:577-609`
  - `mobile-app/app/src/main/java/com/luno/mobile/ui/components/MiniPlayer.kt:67-105`
  - `mobile-app/app/src/main/java/com/luno/mobile/ui/shell/MainShell.kt:1350-1383`
- User behavior: Desktop uses custom thin scrollbars, bounded mouse/legacy-wheel/Tk9 touchpad scrolling, virtualized canvas rows, and a Song Page whose entire content column scrolls and centers when it fits. Desktop controls expose hover, active/drag, selection, playing, favorite, and loading states; `RoundButton`, `SlimSlider`, and `ModernScrollbar` implement these in canvas primitives. Android uses LazyColumn/LazyRow for vertical and horizontal content, a 64 dp swipeable mini-player with a 2 dp progress bar, no-ripple tappable shell controls, and a route transition mask with an opaque green spinner then fade-out.
- State and data flow: Desktop scroll callbacks clamp/ignore wheel movement when content fits and redraw visible rows; Song Page saves/restores scroll position across layout changes. Android lazy content is driven by Room/repository state and shell navigation; transition visibility is stateful and bounded by hold/fade/max timings (`MainShell.kt:127-136`).
- Persistence and compatibility: Scroll offsets and hover/drag states are session UI state only. Queue/playback semantics behind playing indicators and mini-player swipes are behavioral concerns and must remain wired to the existing engine/Media3 controllers.
- Async, lifecycle, and failure behavior: Desktop spinner cancels its `after` loop on destroy and pauses while unmapped (`widgets.py:35-54`); slider redraw is non-destructive and supports motion plus release callbacks (`widgets.py:148-237`). Android transition loading prevents route flash, while artwork and recommendation loading states remain visible through fallback components.
- UI states and visual requirements: Preserve bounded/clamped scroll behavior, 56 px desktop rows, scrollbar hover/drag/page-jump states, hover color changes, slider thumb-on-hover, playing equalizer/pulse, selection highlighting, and actionable loading/error states. Preserve Android lazy scrolling, 48 dp touch targets, mini-player swipe threshold, and opaque route transition behavior.
- Migration implementation requirements: Implement responsive layout breakpoints from observed behavior rather than copying desktop pixel coordinates. Maintain keyboard/mouse affordances on desktop and touch/long-press/swipe affordances on Android. Ensure async state updates target the affected row/component and do not reset scroll or flash the whole page.
- Verification requirements: Exercise wheel, Button-4/5, Tk9 touchpad, scrollbar drag/page jump, slider drag/release, resize with short and overflowing Song Page content, Android horizontal mini-player swipes, route transitions, and lazy-list flings. Current tests cover deterministic artwork/collage logic but no full visual interaction snapshots.
- Confidence: high
- Open questions: No committed manual visual-verification receipt, emulator screenshot set, accessibility audit, or cross-platform pixel-diff baseline exists.

## Migration Decision
- Status: documented-only
- Evidence:
  - `migration-audit/00-repository-census.md:148-180`
  - `migration-audit/00-repository-census.md:158-177`
  - `project_brain.md:257-273`
- User behavior: The target React/TypeScript + Tauri/Rust migration is proposed, not implemented in this repository; no target `src/` or `src-tauri/` visual system is available to audit.
- State and data flow: The current visual authority remains the Python/Tk desktop source, with Android as an implemented parity reference where explicitly useful. Neither runtime shares visual code or tokens.
- Persistence and compatibility: Visual migration must not change the shared `luno.library.export` v1 wire contract or introduce UI-only fields into it.
- Async, lifecycle, and failure behavior: Target implementation must retain desktop event/thread boundaries and equivalent visible loading/failure transitions; planned Rust/TypeScript mechanisms are acceptance requirements, not implementation evidence.
- UI states and visual requirements: Port the desktop shell/layout hierarchy first, then selectively use Android's touch-oriented token and artwork conventions where parity is intentional. Record any deliberate token divergence.
- Migration implementation requirements: Establish target tokens for color, typography, spacing, radii, artwork sizing, row height, scrollbar, and interaction states before screen-by-screen implementation. Include responsive desktop and mobile layouts and explicit visual states for empty/loading/error/hover/selected/playing/download progress.
- Verification requirements: Add a target screenshot/visual regression strategy and manual matrix covering desktop narrow/wide, Android baseline device, long metadata, absent artwork, loading/error states, overflow scrolling, and active playback/download states. Existing Python/Gradle gates validate behavior/build, not visual fidelity.
- Confidence: high
- Open questions: Target framework, exact font, breakpoints, visual regression tooling, and canonical cross-platform token values remain unresolved in source.

## Audit Receipt
- Files inspected: `migration-audit/README.md`, `migration-audit/00-repository-census.md`, `project_brain.md`, `mobile-app/project_brain.md`, `theme.py`, `widgets.py`, `utils.py`, `music_player.py`, `views/library.py`, `views/recent.py`, `views/song_page.py`, `rec_artwork.py`, Android `ui/theme/{Color,Type,Dimens,Theme}.kt`, `ui/shell/MainShell.kt`, `ui/components/{ArtworkImage,ArtworkCollage,MiniPlayer,RecommendationArtwork}.kt`, selected Android screen sources, Android XML colors/themes/drawables, and Android UI tests.
- Tests/fixtures inspected: `mobile-app/app/src/test/java/com/luno/mobile/ui/components/RecommendationArtworkTest.kt`, `ArtworkCollageTest.kt`; source-level desktop virtual-list and interaction contracts; no screenshot fixtures.
- Claims requiring review: Canonical desktop-versus-Android background/accent token choice; target font and breakpoints; exact artwork crop/radius equivalence; whether Android touch conventions should be adopted by desktop migration.
- Coverage gaps: No target migration UI exists; no screenshot/golden-image tests; no emulator/device visual receipt; no accessibility/contrast report; no committed manual resize matrix.
- Status: complete
