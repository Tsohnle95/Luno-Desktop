package com.luno.mobile.ui.components

import android.net.Uri
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.size
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.MusicNote
import androidx.compose.material3.Icon
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import coil.compose.AsyncImagePainter
import coil.compose.rememberAsyncImagePainter
import com.luno.mobile.data.artwork.ArtworkStorage
import com.luno.mobile.ui.theme.Dimens
import com.luno.mobile.ui.theme.PrimaryBackground
import com.luno.mobile.ui.theme.PrimaryText
import com.luno.mobile.ui.theme.SurfaceDark
import com.luno.mobile.ui.theme.SurfaceElevated
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/**
 * Renders cached embedded artwork (a `file://` URI produced by
 * [ArtworkStorage]) with a gradient + music-note placeholder while
 * loading or when the track has no artwork.
 *
 * [decodeSizePx] caps the Coil decode resolution (Coil's default is the
 * original image size, and every row decoding full 512px art while
 * scrolling causes memory churn + GC jank).  The stored artwork is at
 * most 512px, so the default never downscales; thumbnail call sites pass
 * a smaller cap so rows decode only what they display.
 */
@Composable
fun ArtworkImage(
    artworkUri: String?,
    modifier: Modifier = Modifier,
    shape: Shape = androidx.compose.foundation.shape.RoundedCornerShape(Dimens.cornerMedium),
    placeholderIconSize: Dp = 40.dp,
    decodeSizePx: Int = 512,
    onError: (() -> Unit)? = null
) {
    if (artworkUri.isNullOrBlank()) {
        ArtworkPlaceholder(modifier = modifier.clip(shape), iconSize = placeholderIconSize)
        return
    }
    val context = LocalContext.current
    val request = remember(artworkUri, decodeSizePx) {
        coil.request.ImageRequest.Builder(context)
            .data(artworkUri)
            .size(width = decodeSizePx, height = decodeSizePx)
            .build()
    }
    // SubcomposeAsyncImage adds a second composition pass for every state
    // change. That cost is noticeable when a lazy list creates several rows
    // during a fling. Render the placeholder only until a regular painter has
    // a decoded image, so loaded artwork does not redraw the fallback.
    val painter = rememberAsyncImagePainter(model = request)
    val painterState = painter.state
    LaunchedEffect(artworkUri, painterState) {
        if (painterState is AsyncImagePainter.State.Error) onError?.invoke()
    }
    Box(modifier = modifier.clip(shape)) {
        if (painterState is AsyncImagePainter.State.Success) {
            Image(
                painter = painter,
                contentDescription = null,
                modifier = Modifier.fillMaxSize(),
                contentScale = ContentScale.Crop
            )
        } else {
            ArtworkPlaceholder(
                modifier = Modifier.fillMaxSize(),
                iconSize = placeholderIconSize
            )
        }
    }
}

/**
 * Animated gradient colors derived from the artwork's dominant color.
 *
 * Returns `(top, bottom)` stops for a vertical gradient that bleeds into
 * [PrimaryBackground].  Colors animate subtly (600ms) when the artwork
 * changes, satisfying the "gradients animate subtly on transition" spec.
 * Falls back to the static surface gradient when there is no artwork.
 */
@Composable
fun rememberArtworkColors(artworkUri: String?): Pair<Color, Color> {
    val context = LocalContext.current
    var dominantArgb by remember(artworkUri) { mutableStateOf<Int?>(null) }

    LaunchedEffect(artworkUri) {
        dominantArgb = if (artworkUri.isNullOrBlank()) {
            null
        } else {
            withContext(Dispatchers.IO) {
                ArtworkStorage.dominantColor(context, Uri.parse(artworkUri))
            }
        }
    }

    val top by animateColorAsState(
        targetValue = dominantArgb?.let(::Color) ?: SurfaceDark,
        animationSpec = tween(durationMillis = 600),
        label = "artworkGradientTop"
    )
    val bottom by animateColorAsState(
        targetValue = dominantArgb?.let { lerp(Color(it), PrimaryBackground, 0.65f) }
            ?: SurfaceElevated,
        animationSpec = tween(durationMillis = 600),
        label = "artworkGradientBottom"
    )
    return top to bottom
}

@Composable
private fun ArtworkPlaceholder(
    modifier: Modifier = Modifier,
    iconSize: Dp
) {
    Box(
        modifier = modifier.background(
            Brush.linearGradient(colors = listOf(SurfaceDark, SurfaceElevated))
        ),
        contentAlignment = Alignment.Center
    ) {
        Icon(
            imageVector = Icons.Filled.MusicNote,
            contentDescription = null,
            tint = PrimaryText.copy(alpha = 0.3f),
            modifier = Modifier.size(iconSize)
        )
    }
}
