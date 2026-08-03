package com.luno.mobile.ui.discover

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.MaterialTheme
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
import androidx.compose.ui.unit.dp
import com.luno.mobile.ui.theme.AccentGreen
import com.luno.mobile.ui.theme.Dimens
import com.luno.mobile.ui.theme.PrimaryBackground
import com.luno.mobile.ui.theme.PrimaryText
import com.luno.mobile.ui.theme.SecondaryText
import com.luno.mobile.ui.theme.SurfaceDark
import com.luno.mobile.ui.theme.SurfaceElevated

/**
 * Shared Last.fm API-key dialog (opened from the Discover screen's empty
 * state and the Settings drawer).  The key is stored encrypted by
 * [com.luno.mobile.data.repository.DiscoveryRepository].
 */
@Composable
fun LastfmKeyDialog(
    currentKey: String?,
    onSave: (String) -> Unit,
    onClear: () -> Unit,
    onDismiss: () -> Unit
) {
    var input by rememberSaveable { mutableStateOf(currentKey ?: "") }

    AlertDialog(
        onDismissRequest = onDismiss,
        containerColor = SurfaceDark,
        titleContentColor = PrimaryText,
        textContentColor = SecondaryText,
        title = {
            Text(
                text = "Last.fm API key",
                style = MaterialTheme.typography.headlineMedium,
                color = PrimaryText
            )
        },
        text = {
            Column(modifier = Modifier.fillMaxWidth()) {
                Text(
                    text = "Discover fetches recommendations from Last.fm with your own free key " +
                        "— register one at last.fm/api. The key is stored encrypted and never " +
                        "leaves your device.",
                    style = MaterialTheme.typography.bodySmall,
                    color = SecondaryText
                )
                Spacer(modifier = Modifier.height(Dimens.paddingMedium))
                OutlinedTextField(
                    value = input,
                    onValueChange = { input = it },
                    label = { Text("API key") },
                    placeholder = { Text("Paste your Last.fm API key") },
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
                    )
                )
            }
        },
        confirmButton = {
            Button(
                onClick = { onSave(input) },
                colors = ButtonDefaults.buttonColors(
                    containerColor = AccentGreen,
                    contentColor = PrimaryBackground
                ),
                enabled = input.isNotBlank(),
                shape = RoundedCornerShape(Dimens.cornerMedium)
            ) {
                Text("Save")
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) {
                Text("Cancel", color = SecondaryText)
            }
            if (currentKey != null) {
                Spacer(modifier = Modifier.width(Dimens.paddingSmall))
                TextButton(onClick = onClear) {
                    Text("Remove", color = MaterialTheme.colorScheme.error)
                }
            }
        }
    )
}
