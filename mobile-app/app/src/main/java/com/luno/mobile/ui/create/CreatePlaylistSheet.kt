package com.luno.mobile.ui.create

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import com.luno.mobile.LunoApp
import com.luno.mobile.data.db.entity.Playlist
import com.luno.mobile.ui.theme.AccentGreen
import com.luno.mobile.ui.theme.Dimens
import com.luno.mobile.ui.theme.PrimaryBackground
import com.luno.mobile.ui.theme.PrimaryText
import com.luno.mobile.ui.theme.SecondaryText
import com.luno.mobile.ui.theme.SurfaceDark
import com.luno.mobile.ui.theme.SurfaceElevated
import kotlinx.coroutines.launch

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun CreatePlaylistSheet(
    onDismiss: () -> Unit,
    onCreated: (Playlist) -> Unit = {}
) {
    val context = LocalContext.current
    val app = context.applicationContext as LunoApp
    val scope = rememberCoroutineScope()

    var playlistName by remember { mutableStateOf("") }
    var errorMessage by remember { mutableStateOf<String?>(null) }
    var isCreating by remember { mutableStateOf(false) }

    AlertDialog(
        onDismissRequest = onDismiss,
        containerColor = SurfaceDark,
        titleContentColor = PrimaryText,
        textContentColor = SecondaryText,
        title = {
            Text(
                text = "Create Playlist",
                style = MaterialTheme.typography.headlineMedium,
                color = PrimaryText
            )
        },
        text = {
            Column(
                modifier = Modifier.fillMaxWidth()
            ) {
                OutlinedTextField(
                    value = playlistName,
                    onValueChange = {
                        playlistName = it
                        errorMessage = null
                    },
                    label = { Text("Playlist name") },
                    placeholder = { Text("My Playlist") },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth(),
                    colors = OutlinedTextFieldDefaults.colors(
                        focusedTextColor = PrimaryText,
                        unfocusedTextColor = PrimaryText,
                        cursorColor = AccentGreen,
                        focusedBorderColor = AccentGreen,
                        unfocusedBorderColor = SurfaceElevated,
                        focusedContainerColor = PrimaryBackground,
                        unfocusedContainerColor = PrimaryBackground,
                        focusedLabelColor = AccentGreen,
                        unfocusedLabelColor = SecondaryText
                    ),
                    isError = errorMessage != null,
                    supportingText = errorMessage?.let {
                        { Text(text = it, color = MaterialTheme.colorScheme.error) }
                    }
                )

                Spacer(modifier = Modifier.height(Dimens.paddingMedium))
            }
        },
        confirmButton = {
            Button(
                onClick = {
                    if (playlistName.isBlank()) {
                        errorMessage = "Name cannot be empty"
                        return@Button
                    }
                    isCreating = true
                    scope.launch {
                        val result = app.playlistRepository.createPlaylist(playlistName.trim())
                        val playlist = result.getOrNull()
                        if (playlist != null) {
                            onCreated(playlist)
                            onDismiss()
                        } else {
                            errorMessage = result.exceptionOrNull()?.message ?: "Failed to create"
                        }
                        isCreating = false
                    }
                },
                colors = ButtonDefaults.buttonColors(
                    containerColor = AccentGreen,
                    contentColor = PrimaryBackground
                ),
                enabled = !isCreating,
                shape = RoundedCornerShape(Dimens.cornerMedium)
            ) {
                Text(if (isCreating) "Creating..." else "Create")
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) {
                Text("Cancel", color = SecondaryText)
            }
        }
    )
}
