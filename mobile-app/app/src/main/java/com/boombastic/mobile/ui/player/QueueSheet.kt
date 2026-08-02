package com.boombastic.mobile.ui.player

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.boombastic.mobile.playback.MusicController
import com.boombastic.mobile.ui.theme.Dimens
import com.boombastic.mobile.ui.theme.PrimaryText
import com.boombastic.mobile.ui.theme.SurfaceDark

/**
 * Queue bottom sheet — now tabbed (desktop Queue/Recents contract):
 * "Playing Next" (live queue with drag-to-reorder) and "Recently played"
 * (persisted history).  The same tab content is available as a full
 * screen from the Settings drawer ([RecentsScreen]).
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun QueueSheet(
    musicController: MusicController,
    onDismiss: () -> Unit
) {
    var selectedTab by rememberSaveable { mutableStateOf(QueueRecentsTab.PLAYING_NEXT) }

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        containerColor = SurfaceDark
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = Dimens.paddingLarge)
        ) {
            Text(
                text = "Queue",
                style = MaterialTheme.typography.headlineMedium,
                color = PrimaryText
            )
            Spacer(modifier = Modifier.height(Dimens.paddingMedium))
            QueueRecentsTabRow(
                selected = selectedTab,
                onSelect = { selectedTab = it }
            )
            Spacer(modifier = Modifier.height(Dimens.paddingMedium))
            when (selectedTab) {
                QueueRecentsTab.PLAYING_NEXT -> PlayingNextTab(
                    musicController,
                    modifier = Modifier.fillMaxWidth().heightIn(max = 400.dp)
                )
                QueueRecentsTab.RECENTLY_PLAYED -> RecentlyPlayedTab(
                    musicController,
                    modifier = Modifier.fillMaxWidth().heightIn(max = 400.dp)
                )
            }
            Spacer(modifier = Modifier.height(Dimens.paddingXLarge))
        }
    }
}
