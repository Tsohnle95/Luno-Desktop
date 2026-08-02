package com.boombastic.mobile.ui.components

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.RadioButtonUnchecked
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.boombastic.mobile.data.db.entity.Track
import com.boombastic.mobile.ui.theme.AccentGreen
import com.boombastic.mobile.ui.theme.Dimens
import com.boombastic.mobile.ui.theme.PrimaryText
import com.boombastic.mobile.ui.theme.SecondaryText

/**
 * Track row following the playlist-card UI layout: artwork thumbnail on
 * the left, title/artist beside it, green 3-dot options on the right,
 * flat on the screen background.  When [selected] is non-null the row is
 * in multi-select mode and shows a check indicator.
 */
@Composable
@OptIn(ExperimentalFoundationApi::class)
fun TrackRowCard(
    track: Track,
    onClick: () -> Unit,
    onMenuClick: () -> Unit,
    modifier: Modifier = Modifier,
    selected: Boolean? = null,
    onLongClick: (() -> Unit)? = null
) {
    val rowModifier = modifier
            .fillMaxWidth()
            .padding(Dimens.paddingSmall)
    Row(
        modifier = rowModifier.then(
            if (onLongClick != null) {
                Modifier.combinedClickable(
                    interactionSource = remember { MutableInteractionSource() },
                    indication = null,
                    onClick = onClick,
                    onLongClick = onLongClick
                )
            } else {
                Modifier.clickable(
                    interactionSource = remember { MutableInteractionSource() },
                    indication = null,
                    onClick = onClick
                )
            }
        ),
        verticalAlignment = Alignment.CenterVertically
    ) {
        if (selected != null) {
            Icon(
                imageVector = if (selected) {
                    Icons.Filled.CheckCircle
                } else {
                    Icons.Filled.RadioButtonUnchecked
                },
                contentDescription = if (selected) "Selected" else "Not selected",
                tint = if (selected) AccentGreen else SecondaryText,
                modifier = Modifier.size(Dimens.iconSize)
            )
            Spacer(modifier = Modifier.width(Dimens.paddingSmall))
        }

        ArtworkImage(
            artworkUri = track.albumArtUri(),
            modifier = Modifier
                .size(Dimens.albumArtSmall)
                .clip(RoundedCornerShape(Dimens.cornerSmall)),
            placeholderIconSize = 20.dp,
            decodeSizePx = 192
        )

        Spacer(modifier = Modifier.width(Dimens.paddingSmall))

        Column(modifier = Modifier.weight(1f)) {
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

        Box {
            IconButton(onClick = onMenuClick) {
                Icon(
                    imageVector = Icons.Default.MoreVert,
                    contentDescription = "Track options",
                    tint = AccentGreen,
                    modifier = Modifier.size(Dimens.iconSize)
                )
            }
        }
    }
}
