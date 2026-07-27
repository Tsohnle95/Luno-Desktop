package com.boombastic.mobile.ui.library

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import com.boombastic.mobile.BoomBasticApp
import com.boombastic.mobile.data.db.entity.Track
import com.boombastic.mobile.playback.MusicController
import com.boombastic.mobile.ui.home.SectionHeader
import com.boombastic.mobile.ui.theme.Dimens
import com.boombastic.mobile.ui.theme.PrimaryText
import com.boombastic.mobile.ui.theme.SecondaryText

@Composable
fun LibraryScreen(
    musicController: MusicController,
    onPlay: (uri: String) -> Unit = {}
) {
    val context = LocalContext.current
    val app = context.applicationContext as BoomBasticApp
    val allTracks by app.libraryRepository.getAllTracks().collectAsState(initial = emptyList())
    val playlists by app.playlistRepository.getAllPlaylists().collectAsState(initial = emptyList())

    LazyColumn(
        modifier = Modifier
            .fillMaxSize()
            .padding(horizontal = Dimens.paddingLarge),
        verticalArrangement = Arrangement.spacedBy(Dimens.paddingMedium),
        contentPadding = PaddingValues(vertical = Dimens.paddingLarge)
    ) {
        item {
            Text(
                text = "Your Library",
                style = androidx.compose.material3.MaterialTheme.typography.headlineLarge,
                color = PrimaryText,
                modifier = Modifier.padding(bottom = Dimens.paddingMedium)
            )
        }

        // Playlists section
        item {
            SectionHeader(title = "Playlists (${playlists.size})")
        }

        if (playlists.isEmpty()) {
            item {
                Text(
                    text = "No playlists yet. Create one from the Create tab.",
                    color = SecondaryText,
                    style = androidx.compose.material3.MaterialTheme.typography.bodyMedium,
                    modifier = Modifier.padding(vertical = Dimens.paddingMedium)
                )
            }
        } else {
            items(playlists, key = { it.id }) { playlist ->
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(vertical = Dimens.paddingSmall)
                ) {
                    Column {
                        Text(
                            text = playlist.name,
                            style = androidx.compose.material3.MaterialTheme.typography.titleSmall,
                            color = PrimaryText
                        )
                        if (playlist.description.isNotBlank()) {
                            Text(
                                text = playlist.description,
                                style = androidx.compose.material3.MaterialTheme.typography.bodySmall,
                                color = SecondaryText
                            )
                        }
                    }
                }
            }
        }

        // Tracks section
        item {
            SectionHeader(title = "Tracks (${allTracks.size})")
        }

        if (allTracks.isEmpty()) {
            item {
                Text(
                    text = "Import audio files from the Search tab to build your library.",
                    color = SecondaryText,
                    style = androidx.compose.material3.MaterialTheme.typography.bodyMedium,
                    modifier = Modifier.padding(vertical = Dimens.paddingMedium)
                )
            }
        } else {
            items(allTracks, key = { it.uri }) { track ->
                TrackRow(
                    track = track,
                    onClick = { onPlay(track.uri) }
                )
            }
        }
    }
}

@Composable
private fun TrackRow(
    track: Track,
    onClick: () -> Unit
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(onClick = onClick)
            .padding(vertical = Dimens.paddingSmall),
        verticalAlignment = Alignment.CenterVertically
    ) {
        Column(modifier = Modifier.weight(1f)) {
            Text(
                text = track.title,
                style = androidx.compose.material3.MaterialTheme.typography.titleSmall,
                color = PrimaryText,
                maxLines = 1
            )
            Text(
                text = track.artist,
                style = androidx.compose.material3.MaterialTheme.typography.bodySmall,
                color = SecondaryText,
                maxLines = 1
            )
        }
    }
}
