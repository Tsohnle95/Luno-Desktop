package com.luno.mobile.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.luno.mobile.data.db.entity.Track
import com.luno.mobile.ui.theme.Dimens
import com.luno.mobile.ui.theme.SurfaceDark

/**
 * Spotify/desktop-home-style square: the first four track artworks each
 * occupy one quadrant of the square (2x2 collage).  Missing cells render
 * the standard artwork placeholder.  Callers supply the width/size via
 * [modifier] (e.g. `fillMaxWidth()` or `size(48.dp)`); the collage always
 * stays square.
 */
@Composable
fun ArtworkCollage(
    tracks: List<Track>,
    modifier: Modifier = Modifier,
    placeholderIconSize: Dp = 36.dp,
    decodeSizePx: Int = 512,
    shape: androidx.compose.ui.graphics.Shape = RoundedCornerShape(Dimens.cornerSmall)
) {
    Column(
        modifier = modifier
            .aspectRatio(1f)
            .clip(shape)
            .background(SurfaceDark)
    ) {
        for (row in 0 until 2) {
            Row(modifier = Modifier.weight(1f)) {
                for (col in 0 until 2) {
                    val index = row * 2 + col
                    val track = tracks.getOrNull(index)
                    Box(
                        modifier = Modifier
                            .weight(1f)
                            .fillMaxSize(),
                        contentAlignment = Alignment.Center
                    ) {
                        ArtworkImage(
                            artworkUri = track?.albumArtUri(),
                            modifier = Modifier.fillMaxSize(),
                            placeholderIconSize = placeholderIconSize,
                            decodeSizePx = decodeSizePx
                        )
                    }
                }
            }
        }
    }
}
