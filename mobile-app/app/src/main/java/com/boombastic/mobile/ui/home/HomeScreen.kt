package com.boombastic.mobile.ui.home

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Person
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import com.boombastic.mobile.playback.MusicController
import com.boombastic.mobile.ui.components.ArtworkImage
import com.boombastic.mobile.ui.theme.AccentGreen
import com.boombastic.mobile.ui.theme.Dimens
import com.boombastic.mobile.ui.theme.PrimaryBackground
import com.boombastic.mobile.ui.theme.PrimaryText
import com.boombastic.mobile.ui.theme.SecondaryText
import com.boombastic.mobile.ui.theme.SurfaceDark
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

@Composable
fun HomeScreen(
    musicController: MusicController,
    onOpenOptions: () -> Unit = {}
) {
    val currentTrack by musicController.currentTrack.collectAsState()

    val greeting = getGreeting()
    val displayName = "Listener" // Editable in future

    LazyColumn(
        modifier = Modifier
            .fillMaxSize()
            .padding(horizontal = Dimens.paddingLarge),
        verticalArrangement = Arrangement.spacedBy(Dimens.paddingMedium),
        contentPadding = PaddingValues(vertical = Dimens.paddingLarge)
    ) {
        // Greeting header
        item {
            Column {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    // Profile/options entry point (top-left, green circle) —
                    // opens the local-function drawer (downloads, settings,
                    // about, ...)
                    IconButton(
                        onClick = onOpenOptions,
                        modifier = Modifier
                            .size(Dimens.touchTargetMin)
                            .clip(CircleShape)
                            .background(AccentGreen)
                    ) {
                        Icon(
                            imageVector = Icons.Filled.Person,
                            contentDescription = "Options",
                            tint = PrimaryBackground,
                            modifier = Modifier.size(Dimens.iconSize)
                        )
                    }
                    Spacer(modifier = Modifier.weight(1f))
                }
                Text(
                    text = "Good $greeting",
                    style = androidx.compose.material3.MaterialTheme.typography.bodyLarge,
                    color = SecondaryText
                )
                Text(
                    text = displayName,
                    style = androidx.compose.material3.MaterialTheme.typography.headlineLarge,
                    color = PrimaryText
                )
            }
        }

        // Recently played section
        item {
            SectionHeader(title = "Recently played")
        }

        if (currentTrack != null) {
            item {
                LazyRow(
                    horizontalArrangement = Arrangement.spacedBy(Dimens.paddingMedium)
                ) {
                    // Show current track as recently played
                    item {
                        TrackCard(
                            title = currentTrack?.title ?: "Unknown",
                            artist = currentTrack?.artist ?: "Unknown",
                            artworkUri = currentTrack?.artworkUri
                        )
                    }
                }
            }
        } else {
            item {
                EmptyStateCard(
                    title = "No tracks yet",
                    subtitle = "Import audio from the Search tab to get started"
                )
            }
        }

        // Import hint
        item {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(vertical = Dimens.paddingLarge)
            ) {
                Text(
                    text = "Import Music",
                    style = androidx.compose.material3.MaterialTheme.typography.titleLarge,
                    color = PrimaryText
                )
                Text(
                    text = "Use the search tab to import audio files from your device",
                    style = androidx.compose.material3.MaterialTheme.typography.bodyMedium,
                    color = SecondaryText,
                    modifier = Modifier.padding(top = Dimens.paddingSmall)
                )
            }
        }
    }
}

@Composable
fun SectionHeader(title: String) {
    Text(
        text = title,
        style = androidx.compose.material3.MaterialTheme.typography.titleMedium,
        color = PrimaryText,
        fontWeight = FontWeight.Bold,
        modifier = Modifier.padding(vertical = Dimens.paddingSmall)
    )
}

@Composable
fun TrackCard(
    title: String,
    artist: String,
    artworkUri: String? = null
) {
    Column(
        modifier = Modifier
            .padding(top = Dimens.paddingSmall)
    ) {
        // Album art (120dp rounded square), real artwork when available
        ArtworkImage(
            artworkUri = artworkUri,
            modifier = Modifier
                .size(Dimens.albumArtMedium)
                .clip(RoundedCornerShape(Dimens.cornerLarge)),
            placeholderIconSize = 40.dp
        )
        Spacer(modifier = Modifier.height(Dimens.paddingSmall))
        Text(
            text = title,
            style = androidx.compose.material3.MaterialTheme.typography.titleSmall,
            color = PrimaryText,
            maxLines = 1
        )
        Text(
            text = artist,
            style = androidx.compose.material3.MaterialTheme.typography.bodySmall,
            color = SecondaryText,
            maxLines = 1
        )
    }
}

@Composable
fun EmptyStateCard(
    title: String,
    subtitle: String
) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(Dimens.paddingLarge)
    ) {
        Text(
            text = title,
            style = androidx.compose.material3.MaterialTheme.typography.titleMedium,
            color = PrimaryText
        )
        Text(
            text = subtitle,
            style = androidx.compose.material3.MaterialTheme.typography.bodyMedium,
            color = SecondaryText,
            modifier = Modifier.padding(top = Dimens.paddingSmall)
        )
    }
}

private fun getGreeting(): String {
    val hour = SimpleDateFormat("HH", Locale.getDefault()).format(Date()).toIntOrNull() ?: 12
    return when (hour) {
        in 5..11 -> "morning"
        in 12..17 -> "afternoon"
        else -> "evening"
    }
}
