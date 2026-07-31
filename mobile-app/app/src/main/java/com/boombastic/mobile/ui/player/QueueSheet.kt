package com.boombastic.mobile.ui.player

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.boombastic.mobile.playback.MediaTrack
import com.boombastic.mobile.playback.MusicController
import com.boombastic.mobile.ui.theme.Dimens
import com.boombastic.mobile.ui.theme.PrimaryText
import com.boombastic.mobile.ui.theme.SecondaryText
import com.boombastic.mobile.ui.theme.SurfaceDark

/**
 * "Playing Next" bottom sheet backed by the live playback queue from
 * [MusicController.getQueue].  Items are read-only (non-reorderable) for now.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun QueueSheet(
    musicController: MusicController,
    onDismiss: () -> Unit
) {
    val currentTrack by musicController.currentTrack.collectAsState()
    val queueItems = remember(currentTrack) { musicController.getQueue() }

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
                text = "Playing Next",
                style = MaterialTheme.typography.headlineMedium,
                color = PrimaryText
            )
            Spacer(modifier = Modifier.height(Dimens.paddingMedium))

            if (queueItems.isEmpty()) {
                Text(
                    text = "Queue is empty",
                    style = MaterialTheme.typography.bodyMedium,
                    color = SecondaryText,
                    modifier = Modifier.padding(vertical = Dimens.paddingMedium)
                )
            } else {
                LazyColumn(
                    modifier = Modifier
                        .fillMaxWidth()
                        .heightIn(max = 400.dp)
                ) {
                    itemsIndexed(queueItems, key = { index, _ -> index }) { _, track ->
                        QueueRow(track = track)
                    }
                }
            }

            Spacer(modifier = Modifier.height(Dimens.paddingXLarge))
        }
    }
}

@Composable
private fun QueueRow(track: MediaTrack) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = Dimens.paddingSmall)
    ) {
        Text(
            text = track.title,
            style = MaterialTheme.typography.titleSmall,
            color = PrimaryText,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis
        )
        Text(
            text = track.artist,
            style = MaterialTheme.typography.bodySmall,
            color = SecondaryText,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis
        )
    }
}
