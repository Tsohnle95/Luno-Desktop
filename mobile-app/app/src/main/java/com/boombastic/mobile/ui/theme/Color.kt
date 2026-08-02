package com.boombastic.mobile.ui.theme

import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color

// Primary palette
val PrimaryBackground = Color(0xFF101010)
val SurfaceDark = Color(0xFF202020)
val SurfaceElevated = Color(0xFF292929)
val PrimaryText = Color(0xFFFFFFFF)
val SecondaryText = Color(0xFFB3B3B3)
val AccentGreen = Color(0xFF1ED760)

// Two broad, translucent washes of the Home hero's green keep the tint present
// across the viewport. Transparent endpoints let the existing background show
// through instead of creating hard bands.
val AppBackgroundGreen = Color(0xFF102B1C)
val AppBackgroundBrush = Brush.verticalGradient(
    colorStops = arrayOf(
        0.0f to Color.Transparent,
        0.10f to AppBackgroundGreen.copy(alpha = 0.18f),
        0.24f to AppBackgroundGreen.copy(alpha = 0.62f),
        0.38f to AppBackgroundGreen.copy(alpha = 0.16f),
        0.5f to AppBackgroundGreen.copy(alpha = 0.07f),
        0.62f to AppBackgroundGreen.copy(alpha = 0.16f),
        0.76f to AppBackgroundGreen.copy(alpha = 0.62f),
        0.90f to AppBackgroundGreen.copy(alpha = 0.18f),
        1.0f to Color.Transparent
    )
)

// Navigation bar
val NavBarSurface = Color(0xFF181818)
val NavBarSelected = AccentGreen
val NavBarUnselected = Color(0xFFB3B3B3)

// Mini-player
val MiniPlayerSurface = Color(0xFF202020)
val MiniPlayerBorder = Color(0xFF333333)

// Card backgrounds
val CardSurface = Color(0xFF1A1A1A)

// Overlay
val ScrimBackground = Color(0x99000000)

val StatusBarColor = Color(0xFF101010)
val NavigationBarColor = Color(0xFF101010)
