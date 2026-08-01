package com.boombastic.mobile.ui.components

import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import com.boombastic.mobile.data.db.entity.Track
import com.boombastic.mobile.ui.theme.AccentGreen
import com.boombastic.mobile.ui.theme.Dimens
import com.boombastic.mobile.ui.theme.PrimaryText

/**
 * Sort modes shared by the Library and Playlist-detail screens.
 * [DURATION] is longest-first (descending duration).
 */
enum class TrackSortMode(val label: String) {
    AZ("A–Z"),
    ZA("Z–A"),
    RECENT("Recent"),
    DURATION("Duration")
}

/** Sorts a track list by [mode] (DURATION = longest first). */
fun List<Track>.sortedByMode(mode: TrackSortMode): List<Track> = when (mode) {
    TrackSortMode.AZ -> sortedBy { it.title.lowercase() }
    TrackSortMode.ZA -> sortedByDescending { it.title.lowercase() }
    TrackSortMode.RECENT -> sortedByDescending { it.addedAt }
    TrackSortMode.DURATION -> sortedByDescending { it.durationMs }
}

/**
 * The accent-green sort control with its mode dropdown. In [compact] mode it
 * becomes an arrow-only control for dense headers; the selected mode
 * still lives in the caller (rememberSaveable in the screens).
 */
@Composable
fun SortChip(
    mode: TrackSortMode,
    onModeChange: (TrackSortMode) -> Unit,
    compact: Boolean = false
) {
    var showMenu by remember { mutableStateOf(false) }
    Box {
        if (compact) {
            androidx.compose.material3.IconButton(onClick = { showMenu = true }) {
                Icon(
                    imageVector = Icons.Filled.KeyboardArrowDown,
                    contentDescription = "Sort: ${mode.label}",
                    tint = AccentGreen,
                    modifier = Modifier.size(Dimens.iconSize)
                )
            }
        } else {
            Row(
                modifier = Modifier
                    .clip(RoundedCornerShape(Dimens.cornerMedium))
                    .clickable(
                        interactionSource = remember { MutableInteractionSource() },
                        indication = null,
                        onClick = { showMenu = true }
                    )
                    .padding(Dimens.paddingSmall),
                verticalAlignment = Alignment.CenterVertically
            ) {
                Icon(
                    imageVector = Icons.Filled.KeyboardArrowDown,
                    contentDescription = null,
                    tint = AccentGreen,
                    modifier = Modifier.size(Dimens.iconSize)
                )
                Spacer(modifier = Modifier.width(Dimens.paddingSmall))
                Text(
                    text = "Sort: ${mode.label}",
                    style = MaterialTheme.typography.labelLarge,
                    color = AccentGreen
                )
            }
        }
        DropdownMenu(
            expanded = showMenu,
            onDismissRequest = { showMenu = false }
        ) {
            TrackSortMode.entries.forEach { entry ->
                DropdownMenuItem(
                    text = {
                        Text(
                            text = entry.label,
                            color = if (entry == mode) AccentGreen else PrimaryText
                        )
                    },
                    onClick = {
                        showMenu = false
                        onModeChange(entry)
                    }
                )
            }
        }
    }
}
