package com.luno.mobile.ui.components

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import com.luno.mobile.data.db.entity.Track
import com.luno.mobile.ui.theme.AccentGreen
import com.luno.mobile.ui.theme.Dimens
import com.luno.mobile.ui.theme.PrimaryText
import com.luno.mobile.ui.theme.SecondaryText
import com.luno.mobile.ui.theme.SurfaceDark
import com.luno.mobile.ui.theme.SurfaceElevated

/** Editor for imported tracks whose embedded metadata is incomplete. */
@Composable
fun EditTrackMetadataDialog(
    track: Track,
    onDismiss: () -> Unit,
    onSave: (title: String, artist: String) -> Unit
) {
    var title by rememberSaveable(track.uri) { mutableStateOf(track.title) }
    var artist by rememberSaveable(track.uri) { mutableStateOf(track.artist) }

    AlertDialog(
        onDismissRequest = onDismiss,
        containerColor = SurfaceDark,
        titleContentColor = PrimaryText,
        textContentColor = SecondaryText,
        title = { Text("Edit song details") },
        text = {
            Column {
                Text(
                    text = "Add a title and artist so this song can be found in search and recommendations.",
                    style = androidx.compose.material3.MaterialTheme.typography.bodySmall,
                    color = SecondaryText
                )
                Spacer(modifier = Modifier.height(Dimens.paddingMedium))
                OutlinedTextField(
                    value = title,
                    onValueChange = { title = it },
                    modifier = Modifier.fillMaxWidth(),
                    label = { Text("Track name") },
                    singleLine = true,
                    colors = metadataFieldColors()
                )
                Spacer(modifier = Modifier.height(Dimens.paddingSmall))
                OutlinedTextField(
                    value = artist,
                    onValueChange = { artist = it },
                    modifier = Modifier.fillMaxWidth(),
                    label = { Text("Artist name (optional)") },
                    singleLine = true,
                    colors = metadataFieldColors()
                )
            }
        },
        confirmButton = {
            TextButton(
                onClick = { onSave(title.trim(), artist.trim()) },
                enabled = title.trim().isNotBlank()
            ) {
                Text("Save", color = AccentGreen)
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) {
                Text("Cancel", color = SecondaryText)
            }
        }
    )
}

@Composable
private fun metadataFieldColors() = OutlinedTextFieldDefaults.colors(
    focusedTextColor = PrimaryText,
    unfocusedTextColor = PrimaryText,
    focusedLabelColor = AccentGreen,
    unfocusedLabelColor = SecondaryText,
    cursorColor = AccentGreen,
    focusedBorderColor = AccentGreen,
    unfocusedBorderColor = SurfaceElevated,
    focusedContainerColor = SurfaceDark,
    unfocusedContainerColor = SurfaceDark
)
