package com.boombastic.mobile.ui.player

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import com.boombastic.mobile.playback.MusicController
import com.boombastic.mobile.ui.theme.Dimens
import com.boombastic.mobile.ui.theme.PrimaryText
import com.boombastic.mobile.ui.theme.SecondaryText

/**
 * Queue & Recents screen (opened from the Settings drawer) — the same
 * tabbed layout as the full player's [QueueSheet], as a full page:
 * "Playing Next" (live queue, drag-to-reorder) and "Recently played"
 * (persisted history, clear-all).
 */
@Composable
fun RecentsScreen(
    musicController: MusicController,
    onBack: () -> Unit
) {
    var selectedTab by rememberSaveable { mutableStateOf(QueueRecentsTab.PLAYING_NEXT) }

    Column(modifier = Modifier.fillMaxSize()) {
        // Top bar (same pattern as the playlist-detail screen)
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = Dimens.paddingSmall),
            verticalAlignment = Alignment.CenterVertically
        ) {
            IconButton(onClick = onBack) {
                Icon(
                    imageVector = Icons.AutoMirrored.Filled.ArrowBack,
                    contentDescription = "Back",
                    tint = PrimaryText,
                    modifier = Modifier.size(Dimens.iconSize)
                )
            }
            Spacer(modifier = Modifier.weight(1f))
            Text(
                text = "Queue & Recents",
                style = MaterialTheme.typography.titleMedium,
                color = SecondaryText,
                modifier = Modifier.padding(end = Dimens.paddingLarge)
            )
        }

        QueueRecentsTabRow(
            selected = selectedTab,
            onSelect = { selectedTab = it }
        )

        when (selectedTab) {
            QueueRecentsTab.PLAYING_NEXT -> PlayingNextTab(
                musicController,
                modifier = Modifier.fillMaxWidth().weight(1f)
            )
            QueueRecentsTab.RECENTLY_PLAYED -> RecentlyPlayedTab(
                musicController,
                modifier = Modifier.fillMaxWidth().weight(1f)
            )
        }
    }
}
