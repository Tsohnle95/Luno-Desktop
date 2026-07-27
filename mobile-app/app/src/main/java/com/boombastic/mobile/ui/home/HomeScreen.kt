package com.boombastic.mobile.ui.home

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import com.boombastic.mobile.playback.MusicController
import com.boombastic.mobile.ui.theme.AccentGreen
import com.boombastic.mobile.ui.theme.Dimens
import com.boombastic.mobile.ui.theme.PrimaryText
import com.boombastic.mobile.ui.theme.SecondaryText
import com.boombastic.mobile.ui.theme.SurfaceDark
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

@Composable
fun HomeScreen(
    musicController: MusicController
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
                            artist = currentTrack?.artist ?: "Unknown"
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
    artist: String
) {
    Column(
        modifier = Modifier
            .padding(top = Dimens.paddingSmall)
    ) {
        Box(
            modifier = Modifier
                .padding(bottom = Dimens.paddingSmall)
        ) {
            // Placeholder for album art (120dp rounded square)
            Box(
                modifier = Modifier
                    .padding(end = Dimens.paddingSmall)
            ) {
                Text(
                    text = title.firstOrNull()?.toString() ?: "?",
                    color = AccentGreen,
                    style = androidx.compose.material3.MaterialTheme.typography.headlineLarge
                )
            }
        }
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
