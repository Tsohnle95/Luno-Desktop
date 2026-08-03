package com.luno.mobile.ui.theme

import android.app.Activity
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.SideEffect
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.platform.LocalView
import androidx.core.view.WindowCompat

private val LunoColorScheme = darkColorScheme(
    primary = AccentGreen,
    onPrimary = PrimaryBackground,
    secondary = AccentGreen,
    onSecondary = PrimaryBackground,
    background = PrimaryBackground,
    onBackground = PrimaryText,
    surface = SurfaceDark,
    onSurface = PrimaryText,
    surfaceVariant = SurfaceElevated,
    onSurfaceVariant = SecondaryText,
    outline = MiniPlayerBorder
)

@Composable
fun LunoTheme(content: @Composable () -> Unit) {
    val colorScheme = LunoColorScheme
    val view = LocalView.current

    if (!view.isInEditMode) {
        SideEffect {
            val window = (view.context as Activity).window
            // Set status bar and navigation bar colors using the WindowCompat API
            val insetsController = WindowCompat.getInsetsController(window, view)
            insetsController.isAppearanceLightStatusBars = false
            insetsController.isAppearanceLightNavigationBars = false
        }
    }

    MaterialTheme(
        colorScheme = colorScheme,
        typography = LunoTypography,
        content = content
    )
}
