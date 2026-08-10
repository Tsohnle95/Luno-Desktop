package com.luno.mobile.ui.components

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.rotate
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.luno.mobile.data.discovery.LastfmTrack
import com.luno.mobile.data.discovery.normalizedRecommendationKey
import com.luno.mobile.ui.theme.Dimens
import kotlin.math.PI
import kotlin.math.cos
import kotlin.math.sin

/** Number of deterministic artwork designs available to no-art tracks. */
internal const val RECOMMENDATION_ART_DESIGN_COUNT = 75

private data class RecommendationArtPalette(
    val start: Color,
    val end: Color,
    val accent: Color,
    val secondary: Color
)

private val recommendationArtPalettes = listOf(
    RecommendationArtPalette(Color(0xFF25104A), Color(0xFF0A6E8A), Color(0xFFFFB86B), Color(0xFFB8F2E6)),
    RecommendationArtPalette(Color(0xFF401515), Color(0xFFB33B3B), Color(0xFFFFD166), Color(0xFFFF8FA3)),
    RecommendationArtPalette(Color(0xFF102A43), Color(0xFF1D7874), Color(0xFFE8F1F2), Color(0xFFF4A261)),
    RecommendationArtPalette(Color(0xFF20113D), Color(0xFF6A2C70), Color(0xFFFDE74C), Color(0xFF9BC53D)),
    RecommendationArtPalette(Color(0xFF042F2E), Color(0xFF0F766E), Color(0xFF99F6E4), Color(0xFFFBBF24)),
    RecommendationArtPalette(Color(0xFF2E1065), Color(0xFFBE185D), Color(0xFFF9A8D4), Color(0xFFBFDBFE)),
    RecommendationArtPalette(Color(0xFF172554), Color(0xFF1E40AF), Color(0xFF93C5FD), Color(0xFFFDE68A)),
    RecommendationArtPalette(Color(0xFF3B1D0B), Color(0xFFB45309), Color(0xFFFDE68A), Color(0xFFFCA5A5)),
    RecommendationArtPalette(Color(0xFF082F49), Color(0xFF0369A1), Color(0xFFBAE6FD), Color(0xFFA7F3D0)),
    RecommendationArtPalette(Color(0xFF312E81), Color(0xFF7C3AED), Color(0xFFE9D5FF), Color(0xFFFDE047)),
    RecommendationArtPalette(Color(0xFF3F0D12), Color(0xFF8F2D56), Color(0xFFFFC857), Color(0xFFB8F2E6)),
    RecommendationArtPalette(Color(0xFF1C1917), Color(0xFF57534E), Color(0xFFF5F5F4), Color(0xFFF59E0B)),
    RecommendationArtPalette(Color(0xFF052E16), Color(0xFF166534), Color(0xFFBBF7D0), Color(0xFFFDE68A)),
    RecommendationArtPalette(Color(0xFF450A0A), Color(0xFFBE123C), Color(0xFFFDA4AF), Color(0xFFFDE68A)),
    RecommendationArtPalette(Color(0xFF0C4A6E), Color(0xFF155E75), Color(0xFF67E8F9), Color(0xFFF0ABFC)),
    RecommendationArtPalette(Color(0xFF1E1B4B), Color(0xFF3730A3), Color(0xFFC4B5FD), Color(0xFFF9A8D4)),
    RecommendationArtPalette(Color(0xFF422006), Color(0xFF92400E), Color(0xFFFDE68A), Color(0xFFFCA5A5)),
    RecommendationArtPalette(Color(0xFF134E4A), Color(0xFF115E59), Color(0xFF5EEAD4), Color(0xFFFCD34D)),
    RecommendationArtPalette(Color(0xFF171717), Color(0xFF404040), Color(0xFFE5E5E5), Color(0xFFFB7185)),
    RecommendationArtPalette(Color(0xFF172554), Color(0xFF0369A1), Color(0xFFBAE6FD), Color(0xFFF0FDFA)),
    RecommendationArtPalette(Color(0xFF581C87), Color(0xFF9333EA), Color(0xFFF5D0FE), Color(0xFFFDE68A)),
    RecommendationArtPalette(Color(0xFF431407), Color(0xFFEA580C), Color(0xFFFED7AA), Color(0xFFBFDBFE)),
    RecommendationArtPalette(Color(0xFF042F2E), Color(0xFF047857), Color(0xFFA7F3D0), Color(0xFFFDE68A)),
    RecommendationArtPalette(Color(0xFF172554), Color(0xFF4338CA), Color(0xFFC7D2FE), Color(0xFFFECACA)),
    RecommendationArtPalette(Color(0xFF3F0D12), Color(0xFFBE123C), Color(0xFFFBCFE8), Color(0xFFBFDBFE)),
    RecommendationArtPalette(Color(0xFF082F49), Color(0xFF164E63), Color(0xFF67E8F9), Color(0xFFFDE68A)),
    RecommendationArtPalette(Color(0xFF3B0764), Color(0xFF86198F), Color(0xFFF0ABFC), Color(0xFFA7F3D0)),
    RecommendationArtPalette(Color(0xFF052E16), Color(0xFF15803D), Color(0xFF86EFAC), Color(0xFFFDE047)),
    RecommendationArtPalette(Color(0xFF451A03), Color(0xFFB45309), Color(0xFFFDBA74), Color(0xFFBFDBFE)),
    RecommendationArtPalette(Color(0xFF172554), Color(0xFF1D4ED8), Color(0xFF93C5FD), Color(0xFFF9A8D4)),
    RecommendationArtPalette(Color(0xFF4A044E), Color(0xFFBE185D), Color(0xFFF9A8D4), Color(0xFFFDE68A)),
    RecommendationArtPalette(Color(0xFF042F2E), Color(0xFF0D9488), Color(0xFF99F6E4), Color(0xFFFCA5A5)),
    RecommendationArtPalette(Color(0xFF1C1917), Color(0xFF78716C), Color(0xFFE7E5E4), Color(0xFFFBBF24)),
    RecommendationArtPalette(Color(0xFF3F0D12), Color(0xFF9F1239), Color(0xFFFDA4AF), Color(0xFFBAE6FD)),
    RecommendationArtPalette(Color(0xFF0C4A6E), Color(0xFF0284C7), Color(0xFFBAE6FD), Color(0xFFF5D0FE)),
    RecommendationArtPalette(Color(0xFF312E81), Color(0xFF4F46E5), Color(0xFFC4B5FD), Color(0xFFFDE68A)),
    RecommendationArtPalette(Color(0xFF431407), Color(0xFFC2410C), Color(0xFFFED7AA), Color(0xFFA7F3D0)),
    RecommendationArtPalette(Color(0xFF064E3B), Color(0xFF047857), Color(0xFF6EE7B7), Color(0xFFFDE68A)),
    RecommendationArtPalette(Color(0xFF18181B), Color(0xFF52525B), Color(0xFFD4D4D8), Color(0xFFFDA4AF)),
    RecommendationArtPalette(Color(0xFF1E1B4B), Color(0xFF6D28D9), Color(0xFFDDD6FE), Color(0xFFFBCFE8)),
    RecommendationArtPalette(Color(0xFF500724), Color(0xFFBE185D), Color(0xFFFBCFE8), Color(0xFFBFDBFE)),
    RecommendationArtPalette(Color(0xFF083344), Color(0xFF0E7490), Color(0xFF67E8F9), Color(0xFFFDE68A)),
    RecommendationArtPalette(Color(0xFF365314), Color(0xFF65A30D), Color(0xFFD9F99D), Color(0xFFFDBA74)),
    RecommendationArtPalette(Color(0xFF422006), Color(0xFFD97706), Color(0xFFFDE68A), Color(0xFFBFDBFE)),
    RecommendationArtPalette(Color(0xFF0F172A), Color(0xFF334155), Color(0xFFCBD5E1), Color(0xFFF0ABFC)),
    RecommendationArtPalette(Color(0xFF581C87), Color(0xFFA21CAF), Color(0xFFF5D0FE), Color(0xFF99F6E4)),
    RecommendationArtPalette(Color(0xFF052E16), Color(0xFF166534), Color(0xFFBBF7D0), Color(0xFFFCA5A5)),
    RecommendationArtPalette(Color(0xFF450A0A), Color(0xFF991B1B), Color(0xFFFECACA), Color(0xFFFDE68A)),
    RecommendationArtPalette(Color(0xFF164E63), Color(0xFF0369A1), Color(0xFFBAE6FD), Color(0xFFF5D0FE)),
    RecommendationArtPalette(Color(0xFF27272A), Color(0xFF71717A), Color(0xFFF4F4F5), Color(0xFFA7F3D0)),
    RecommendationArtPalette(Color(0xFF1E3A8A), Color(0xFF2563EB), Color(0xFFBFDBFE), Color(0xFFFDE68A)),
    RecommendationArtPalette(Color(0xFF701A75), Color(0xFFC026D3), Color(0xFFF5D0FE), Color(0xFFFDE68A)),
    RecommendationArtPalette(Color(0xFF3F6212), Color(0xFF4D7C0F), Color(0xFFBEF264), Color(0xFFFDBA74)),
    RecommendationArtPalette(Color(0xFF7C2D12), Color(0xFFEA580C), Color(0xFFFED7AA), Color(0xFFBAE6FD)),
    RecommendationArtPalette(Color(0xFF042F2E), Color(0xFF0F766E), Color(0xFF99F6E4), Color(0xFFFBCFE8)),
    RecommendationArtPalette(Color(0xFF172554), Color(0xFF3730A3), Color(0xFFC7D2FE), Color(0xFFFDA4AF)),
    RecommendationArtPalette(Color(0xFF4C0519), Color(0xFF9D174D), Color(0xFFFBCFE8), Color(0xFFFDE68A)),
    RecommendationArtPalette(Color(0xFF022C22), Color(0xFF047857), Color(0xFFA7F3D0), Color(0xFFBFDBFE)),
    RecommendationArtPalette(Color(0xFF292524), Color(0xFF57534E), Color(0xFFE7E5E4), Color(0xFF93C5FD)),
    RecommendationArtPalette(Color(0xFF1E1B4B), Color(0xFF4338CA), Color(0xFFC4B5FD), Color(0xFFFDE68A)),
    RecommendationArtPalette(Color(0xFF431407), Color(0xFF9A3412), Color(0xFFFED7AA), Color(0xFFA7F3D0)),
    RecommendationArtPalette(Color(0xFF082F49), Color(0xFF075985), Color(0xFFBAE6FD), Color(0xFFFCA5A5)),
    RecommendationArtPalette(Color(0xFF3B0764), Color(0xFF7E22CE), Color(0xFFE9D5FF), Color(0xFF99F6E4)),
    RecommendationArtPalette(Color(0xFF14532D), Color(0xFF16A34A), Color(0xFFBBF7D0), Color(0xFFFDE68A)),
    RecommendationArtPalette(Color(0xFF3F0D12), Color(0xFFBE123C), Color(0xFFFDA4AF), Color(0xFFFDBA74)),
    RecommendationArtPalette(Color(0xFF0F172A), Color(0xFF1E40AF), Color(0xFFBFDBFE), Color(0xFFF0ABFC)),
    RecommendationArtPalette(Color(0xFF581C87), Color(0xFF86198F), Color(0xFFF5D0FE), Color(0xFFFDE68A)),
    RecommendationArtPalette(Color(0xFF164E63), Color(0xFF0F766E), Color(0xFF99F6E4), Color(0xFFFCA5A5)),
    RecommendationArtPalette(Color(0xFF365314), Color(0xFF4D7C0F), Color(0xFFD9F99D), Color(0xFFBFDBFE)),
    RecommendationArtPalette(Color(0xFF7C2D12), Color(0xFFC2410C), Color(0xFFFED7AA), Color(0xFFFDE68A)),
    RecommendationArtPalette(Color(0xFF1E293B), Color(0xFF475569), Color(0xFFE2E8F0), Color(0xFFF9A8D4)),
    RecommendationArtPalette(Color(0xFF500724), Color(0xFF9D174D), Color(0xFFFBCFE8), Color(0xFFA7F3D0)),
    RecommendationArtPalette(Color(0xFF064E3B), Color(0xFF0F766E), Color(0xFF99F6E4), Color(0xFFFDE68A)),
    RecommendationArtPalette(Color(0xFF450A0A), Color(0xFFB91C1C), Color(0xFFFECACA), Color(0xFFBFDBFE)),
    RecommendationArtPalette(Color(0xFF172554), Color(0xFF1D4ED8), Color(0xFF93C5FD), Color(0xFFA7F3D0))
)

/** Stable design assignment: the same recommendation always gets the same art. */
internal fun recommendationArtworkDesignIndex(artist: String, title: String): Int =
    Math.floorMod(
        normalizedRecommendationKey(artist, title).hashCode(),
        RECOMMENDATION_ART_DESIGN_COUNT
    )

/**
 * Generates a deterministic abstract cover for a recommendation with no
 * usable remote artwork. It is deliberately code-native so it works offline,
 * scales cleanly from a row thumbnail to the full player, and does not add
 * bitmap resources to the APK. The pool contains 75 designs so ordinary
 * no-art library tracks and recommendations share a varied fallback set.
 */
@Composable
fun RecommendationArtwork(
    artist: String,
    title: String,
    modifier: Modifier = Modifier,
    shape: Shape = RoundedCornerShape(Dimens.cornerMedium)
) {
    val design = remember(artist, title) {
        recommendationArtworkDesignIndex(artist, title)
    }
    val palette = recommendationArtPalettes[design]

    Box(
        modifier = modifier
            .clip(shape)
            .background(Brush.linearGradient(listOf(palette.start, palette.end)))
    ) {
        Canvas(modifier = Modifier.fillMaxSize()) {
            drawRecommendationDesign(design, palette)
        }
    }
}

/**
 * Recommendation-specific image wrapper with abstract art for every non-success
 * image state: missing URL, Coil loading, and Coil failure.
 */
@Composable
fun RecommendationArtworkImage(
    recommendation: LastfmTrack,
    artworkUri: String?,
    modifier: Modifier = Modifier,
    shape: Shape = RoundedCornerShape(Dimens.cornerMedium),
    placeholderIconSize: Dp = 24.dp,
    decodeSizePx: Int = 512,
    onError: (() -> Unit)? = null
) {
    RecommendationArtworkImage(
        artist = recommendation.artist,
        title = recommendation.title,
        artworkUri = artworkUri,
        modifier = modifier,
        shape = shape,
        placeholderIconSize = placeholderIconSize,
        decodeSizePx = decodeSizePx,
        onError = onError
    )
}

/** Same fallback for transient preview tracks represented by playback metadata. */
@Composable
fun RecommendationArtworkImage(
    artist: String,
    title: String,
    artworkUri: String?,
    modifier: Modifier = Modifier,
    shape: Shape = RoundedCornerShape(Dimens.cornerMedium),
    placeholderIconSize: Dp = 24.dp,
    decodeSizePx: Int = 512,
    onError: (() -> Unit)? = null
) {
    ArtworkImage(
        artworkUri = artworkUri,
        modifier = modifier,
        shape = shape,
        placeholderIconSize = placeholderIconSize,
        decodeSizePx = decodeSizePx,
        onError = onError,
        placeholderContent = {
            RecommendationArtwork(
                artist = artist,
                title = title,
                modifier = Modifier.fillMaxSize(),
                shape = shape
            )
        }
    )
}

private fun DrawScope.drawRecommendationDesign(
    design: Int,
    palette: RecommendationArtPalette
) {
    val width = size.width
    val height = size.height
    val side = minOf(width, height)
    val center = Offset(width / 2f, height / 2f)
    val bright = palette.accent.copy(alpha = 0.9f)
    val secondary = palette.secondary.copy(alpha = 0.72f)
    val white = Color.White.copy(alpha = 0.24f)

    if (design >= 25) {
        drawExtendedRecommendationDesign(design - 25, palette)
        return
    }

    when (design) {
        0 -> {
            drawCircle(secondary, side * 0.56f, Offset(width * 0.68f, height * 0.36f))
            drawCircle(bright, side * 0.16f, Offset(width * 0.3f, height * 0.7f))
            drawArc(
                color = white,
                startAngle = 210f,
                sweepAngle = 230f,
                useCenter = false,
                topLeft = Offset(width * 0.08f, height * 0.08f),
                size = Size(side * 0.84f, side * 0.84f),
                style = Stroke(side * 0.045f)
            )
        }
        1 -> {
            rotate(24f, center) {
                drawRoundRect(secondary, Offset(-side * 0.2f, height * 0.32f), Size(width * 1.4f, side * 0.18f), androidx.compose.ui.geometry.CornerRadius(side * 0.08f))
                drawRoundRect(bright, Offset(-side * 0.2f, height * 0.58f), Size(width * 1.4f, side * 0.07f), androidx.compose.ui.geometry.CornerRadius(side * 0.03f))
            }
            for (i in 0..3) {
                drawCircle(white, side * 0.035f, Offset(width * (0.18f + i * 0.2f), height * 0.18f))
            }
        }
        2 -> {
            drawCircle(secondary, side * 0.56f, center)
            drawCircle(palette.start.copy(alpha = 0.85f), side * 0.4f, center)
            drawCircle(bright, side * 0.24f, center)
            drawCircle(palette.start.copy(alpha = 0.8f), side * 0.09f, center)
        }
        3 -> {
            for (i in 1..4) {
                val x = width * i / 5f
                val y = height * i / 5f
                drawLine(white, Offset(x, 0f), Offset(x, height), side * 0.012f)
                drawLine(white, Offset(0f, y), Offset(width, y), side * 0.012f)
            }
            drawLine(bright, Offset(0f, height), Offset(width, 0f), side * 0.07f, cap = StrokeCap.Round)
            drawCircle(secondary, side * 0.14f, Offset(width * 0.72f, height * 0.28f))
        }
        4 -> {
            drawOval(
                color = secondary,
                topLeft = Offset(width * 0.08f, height * 0.28f),
                size = Size(width * 0.84f, height * 0.42f),
                style = Stroke(side * 0.045f)
            )
            drawOval(
                color = white,
                topLeft = Offset(width * 0.2f, height * 0.08f),
                size = Size(width * 0.58f, height * 0.84f),
                style = Stroke(side * 0.022f)
            )
            drawCircle(bright, side * 0.12f, Offset(width * 0.68f, height * 0.34f))
            drawCircle(palette.secondary, side * 0.045f, Offset(width * 0.29f, height * 0.67f))
        }
        5 -> {
            val path = Path().apply {
                moveTo(0f, height * 0.78f)
                lineTo(width * 0.28f, height * 0.35f)
                lineTo(width * 0.46f, height * 0.58f)
                lineTo(width * 0.7f, height * 0.2f)
                lineTo(width, height * 0.68f)
                lineTo(width, height)
                lineTo(0f, height)
                close()
            }
            drawPath(path, secondary)
            drawLine(bright, Offset(0f, height * 0.78f), Offset(width * 0.28f, height * 0.35f), side * 0.035f, cap = StrokeCap.Round)
            drawLine(bright, Offset(width * 0.28f, height * 0.35f), Offset(width * 0.46f, height * 0.58f), side * 0.035f, cap = StrokeCap.Round)
            drawLine(bright, Offset(width * 0.46f, height * 0.58f), Offset(width * 0.7f, height * 0.2f), side * 0.035f, cap = StrokeCap.Round)
        }
        6 -> {
            listOf(
                Offset(width * 0.3f, height * 0.34f) to side * 0.24f,
                Offset(width * 0.67f, height * 0.3f) to side * 0.18f,
                Offset(width * 0.52f, height * 0.68f) to side * 0.3f
            ).forEachIndexed { index, (point, radius) ->
                drawCircle(if (index == 1) bright else secondary, radius, point)
            }
            drawCircle(white, side * 0.08f, Offset(width * 0.3f, height * 0.34f))
        }
        7 -> {
            val pieces = listOf(
                Offset(0.12f, 0.18f), Offset(0.32f, 0.3f), Offset(0.62f, 0.16f),
                Offset(0.8f, 0.38f), Offset(0.18f, 0.68f), Offset(0.46f, 0.84f),
                Offset(0.76f, 0.72f)
            )
            pieces.forEachIndexed { index, fraction ->
                val pieceSize = side * (0.08f + (index % 3) * 0.025f)
                rotate(index * 23f - 28f, Offset(width * fraction.x, height * fraction.y)) {
                    drawRoundRect(
                        if (index % 2 == 0) bright else secondary,
                        Offset(width * fraction.x - pieceSize / 2f, height * fraction.y - pieceSize / 2f),
                        Size(pieceSize, pieceSize * 0.42f),
                        androidx.compose.ui.geometry.CornerRadius(pieceSize * 0.12f)
                    )
                }
            }
        }
        8 -> {
            val wave = Path().apply {
                moveTo(0f, height * 0.58f)
                cubicTo(width * 0.2f, height * 0.18f, width * 0.36f, height * 0.94f, width * 0.58f, height * 0.5f)
                cubicTo(width * 0.76f, height * 0.14f, width * 0.9f, height * 0.74f, width, height * 0.3f)
            }
            drawPath(wave, bright, style = Stroke(width = side * 0.08f, cap = StrokeCap.Round, join = StrokeJoin.Round))
            drawPath(wave, white, style = Stroke(width = side * 0.018f, cap = StrokeCap.Round))
        }
        9 -> {
            val cell = side * 0.2f
            for (row in 0..4) {
                for (column in 0..4) {
                    if ((row + column) % 2 == 0) {
                        drawRect(
                            if (row % 2 == 0) secondary else bright,
                            Offset(width * 0.02f + column * cell, height * 0.02f + row * cell),
                            Size(cell * 0.9f, cell * 0.9f)
                        )
                    }
                }
            }
        }
        10 -> {
            for (i in 0 until 18) {
                val angle = (i / 18f) * (2.0 * PI).toFloat()
                val cosine = cos(angle.toDouble()).toFloat()
                val sine = sin(angle.toDouble()).toFloat()
                val start = Offset(center.x + cosine * side * 0.16f, center.y + sine * side * 0.16f)
                val end = Offset(center.x + cosine * side * 0.6f, center.y + sine * side * 0.6f)
                drawLine(if (i % 3 == 0) bright else white, start, end, side * 0.028f, cap = StrokeCap.Round)
            }
            drawCircle(secondary, side * 0.15f, center)
        }
        11 -> {
            for (row in 0..3) {
                drawRoundRect(
                    if (row % 2 == 0) secondary else bright,
                    Offset(width * (0.08f + row * 0.05f), height * (0.13f + row * 0.2f)),
                    Size(width * 0.75f, height * 0.1f),
                    androidx.compose.ui.geometry.CornerRadius(height * 0.05f)
                )
            }
            drawCircle(white, side * 0.07f, Offset(width * 0.78f, height * 0.72f))
        }
        12 -> {
            drawRect(secondary, Offset(0f, 0f), Size(width * 0.55f, height))
            drawCircle(bright, side * 0.39f, Offset(width * 0.7f, height * 0.32f))
            drawCircle(palette.start.copy(alpha = 0.7f), side * 0.25f, Offset(width * 0.7f, height * 0.32f))
            drawLine(white, Offset(width * 0.55f, 0f), Offset(width * 0.55f, height), side * 0.025f)
        }
        13 -> {
            rotate(45f, center) {
                drawRoundRect(secondary, Offset(width * 0.17f, height * 0.17f), Size(width * 0.66f, height * 0.66f), androidx.compose.ui.geometry.CornerRadius(side * 0.08f))
                drawRoundRect(palette.start.copy(alpha = 0.8f), Offset(width * 0.32f, height * 0.32f), Size(width * 0.36f, height * 0.36f), androidx.compose.ui.geometry.CornerRadius(side * 0.04f))
            }
            drawCircle(bright, side * 0.08f, Offset(width * 0.18f, height * 0.78f))
        }
        14 -> {
            val points = listOf(
                Offset(width * 0.18f, height * 0.68f), Offset(width * 0.34f, height * 0.32f),
                Offset(width * 0.56f, height * 0.58f), Offset(width * 0.78f, height * 0.2f),
                Offset(width * 0.82f, height * 0.78f)
            )
            points.zipWithNext().forEach { (from, to) ->
                drawLine(white, from, to, side * 0.018f)
            }
            points.forEachIndexed { index, point ->
                drawCircle(if (index == 2) bright else secondary, side * (0.055f + index * 0.009f), point)
            }
        }
        15 -> {
            drawArc(
                color = secondary,
                startAngle = 20f,
                sweepAngle = 120f,
                useCenter = false,
                topLeft = Offset(-side * 0.1f, height * 0.12f),
                size = Size(side * 0.95f, side * 0.95f),
                style = Stroke(side * 0.12f, cap = StrokeCap.Round)
            )
            drawArc(
                color = bright,
                startAngle = 160f,
                sweepAngle = 150f,
                useCenter = false,
                topLeft = Offset(width * 0.18f, height * 0.18f),
                size = Size(side * 0.9f, side * 0.9f),
                style = Stroke(side * 0.055f, cap = StrokeCap.Round)
            )
            drawArc(
                color = white,
                startAngle = 275f,
                sweepAngle = 65f,
                useCenter = false,
                topLeft = Offset(width * 0.3f, -side * 0.1f),
                size = Size(side * 0.75f, side * 0.75f),
                style = Stroke(side * 0.025f, cap = StrokeCap.Round)
            )
        }
        16 -> {
            rotate(-35f, center) {
                for (i in -2..5) {
                    drawRect(
                        if (i % 2 == 0) secondary else bright,
                        Offset(width * 0.12f, i * side * 0.22f),
                        Size(width * 0.78f, side * 0.1f)
                    )
                }
            }
            drawCircle(white, side * 0.1f, Offset(width * 0.72f, height * 0.72f))
        }
        17 -> {
            for (i in 0 until 8) {
                val angle = (i / 8f) * (2.0 * PI).toFloat()
                val cosine = cos(angle.toDouble()).toFloat()
                val sine = sin(angle.toDouble()).toFloat()
                drawCircle(
                    if (i % 2 == 0) secondary else bright,
                    side * 0.18f,
                    Offset(center.x + cosine * side * 0.27f, center.y + sine * side * 0.27f)
                )
            }
            drawCircle(palette.start.copy(alpha = 0.9f), side * 0.16f, center)
            drawCircle(white, side * 0.05f, center)
        }
        18 -> {
            val zigzag = Path().apply {
                moveTo(0f, height * 0.72f)
                lineTo(width * 0.2f, height * 0.22f)
                lineTo(width * 0.4f, height * 0.7f)
                lineTo(width * 0.6f, height * 0.18f)
                lineTo(width * 0.8f, height * 0.64f)
                lineTo(width, height * 0.28f)
            }
            drawPath(zigzag, secondary, style = Stroke(width = side * 0.16f, cap = StrokeCap.Round, join = StrokeJoin.Round))
            drawPath(zigzag, bright, style = Stroke(width = side * 0.045f, cap = StrokeCap.Round, join = StrokeJoin.Round))
        }
        19 -> {
            drawRoundRect(secondary, Offset(width * 0.1f, height * 0.1f), Size(width * 0.8f, height * 0.8f), androidx.compose.ui.geometry.CornerRadius(side * 0.08f), style = Stroke(side * 0.065f))
            drawRoundRect(white, Offset(width * 0.28f, height * 0.28f), Size(width * 0.44f, height * 0.44f), androidx.compose.ui.geometry.CornerRadius(side * 0.04f), style = Stroke(side * 0.028f))
            drawLine(bright, Offset(width * 0.5f, height * 0.28f), Offset(width * 0.5f, height * 0.72f), side * 0.035f)
            drawLine(bright, Offset(width * 0.28f, height * 0.5f), Offset(width * 0.72f, height * 0.5f), side * 0.035f)
        }
        20 -> {
            drawLine(white, Offset(width * 0.12f, height * 0.78f), Offset(width * 0.78f, height * 0.2f), side * 0.06f, cap = StrokeCap.Round)
            drawCircle(secondary, side * 0.28f, Offset(width * 0.72f, height * 0.24f))
            drawCircle(palette.start.copy(alpha = 0.9f), side * 0.16f, Offset(width * 0.72f, height * 0.24f))
            drawCircle(bright, side * 0.08f, Offset(width * 0.2f, height * 0.76f))
        }
        21 -> {
            val ribbon = Path().apply {
                moveTo(-width * 0.1f, height * 0.68f)
                cubicTo(width * 0.24f, height * 0.1f, width * 0.62f, height * 0.94f, width * 1.1f, height * 0.3f)
            }
            drawPath(ribbon, secondary, style = Stroke(width = side * 0.22f, cap = StrokeCap.Round))
            drawPath(ribbon, bright, style = Stroke(width = side * 0.06f, cap = StrokeCap.Round))
        }
        22 -> {
            for (row in 0..5) {
                for (column in 0..5) {
                    val radius = side * (0.025f + ((row + column) % 3) * 0.018f)
                    drawCircle(
                        if ((row + column) % 2 == 0) bright else secondary,
                        radius,
                        Offset(width * (0.12f + column * 0.15f), height * (0.12f + row * 0.15f))
                    )
                }
            }
        }
        23 -> {
            drawCircle(secondary, side * 0.46f, Offset(width * 0.42f, height * 0.5f))
            drawCircle(palette.start.copy(alpha = 0.92f), side * 0.46f, Offset(width * 0.58f, height * 0.5f))
            drawCircle(bright, side * 0.13f, Offset(width * 0.42f, height * 0.5f))
            drawCircle(secondary, side * 0.13f, Offset(width * 0.58f, height * 0.5f))
        }
        else -> {
            val spikes = Path()
            for (i in 0 until 16) {
                val angle = (i / 16f) * (2.0 * PI).toFloat() - (PI / 2.0).toFloat()
                val cosine = cos(angle.toDouble()).toFloat()
                val sine = sin(angle.toDouble()).toFloat()
                val radius = if (i % 2 == 0) side * 0.58f else side * 0.22f
                val point = Offset(center.x + cosine * radius, center.y + sine * radius)
                if (i == 0) spikes.moveTo(point.x, point.y) else spikes.lineTo(point.x, point.y)
            }
            spikes.close()
            drawPath(spikes, secondary)
            drawCircle(bright, side * 0.14f, center)
            drawCircle(white, side * 0.04f, center)
        }
    }
}

/**
 * Fifty additional deterministic designs, arranged as ten visual families
 * with five palette/geometry variations each. Keeping these procedural lets
 * the artwork remain crisp at every player size without shipping bitmaps.
 */
private fun DrawScope.drawExtendedRecommendationDesign(
    variant: Int,
    palette: RecommendationArtPalette
) {
    val width = size.width
    val height = size.height
    val side = minOf(width, height)
    val center = Offset(width / 2f, height / 2f)
    val family = variant % 10
    val layer = variant / 10
    val phase = layer * 0.09f
    val bright = palette.accent.copy(alpha = 0.92f)
    val secondary = palette.secondary.copy(alpha = 0.76f)
    val white = Color.White.copy(alpha = 0.22f)

    when (family) {
        0 -> {
            rotate(-30f + layer * 16f, center) {
                for (index in -2..7) {
                    val thickness = side * (0.055f + (index + layer).mod(3) * 0.02f)
                    drawRect(
                        if ((index + layer) % 2 == 0) secondary else bright,
                        Offset(-side * 0.2f, index * side * 0.2f + phase * side),
                        Size(width * 1.4f, thickness)
                    )
                }
            }
        }
        1 -> {
            for (ring in 0..3) {
                val radius = side * (0.12f + ring * (0.11f + layer * 0.012f))
                drawCircle(
                    if ((ring + layer) % 2 == 0) secondary else bright,
                    radius,
                    Offset(center.x + layer * side * 0.018f, center.y - layer * side * 0.012f),
                    style = Stroke(width = side * (0.025f + ring * 0.008f))
                )
            }
            drawCircle(white, side * 0.035f, center)
        }
        2 -> {
            for (orbit in 0..3) {
                val inset = side * (0.08f + orbit * 0.08f)
                drawOval(
                    if ((orbit + layer) % 2 == 0) bright else secondary,
                    topLeft = Offset(inset * 0.5f, inset),
                    size = Size(width - inset, height - inset * 2f),
                    style = Stroke(width = side * (0.018f + layer * 0.004f))
                )
            }
            drawCircle(white, side * 0.07f, Offset(width * (0.25f + phase), height * 0.72f))
        }
        3 -> {
            val shardCount = 4 + layer
            for (shard in 0 until shardCount) {
                val left = width * (shard.toFloat() / shardCount) - side * 0.06f
                val top = height * (0.1f + ((shard + layer) % 3) * 0.18f)
                val path = Path().apply {
                    moveTo(left, height)
                    lineTo(left + side * (0.28f + layer * 0.025f), top)
                    lineTo(left + side * 0.48f, height)
                    close()
                }
                drawPath(path, if ((shard + layer) % 2 == 0) secondary else bright)
            }
            drawLine(white, Offset(0f, height * 0.18f), Offset(width, height * 0.82f), side * 0.018f)
        }
        4 -> {
            for (bar in 0..7) {
                val barHeight = height * (0.12f + ((bar + layer) % 4) * 0.045f)
                val x = width * (0.06f + bar * 0.13f)
                drawRoundRect(
                    if ((bar + layer) % 2 == 0) bright else secondary,
                    Offset(x, center.y - barHeight / 2f + sin(bar + layer.toDouble()).toFloat() * side * 0.08f),
                    Size(width * 0.07f, barHeight),
                    androidx.compose.ui.geometry.CornerRadius(side * 0.03f)
                )
            }
            drawLine(white, Offset(width * 0.04f, center.y), Offset(width * 0.96f, center.y), side * 0.018f)
        }
        5 -> {
            for (row in -1..4) {
                for (column in -1..4) {
                    val x = width * (0.12f + column * 0.2f)
                    val y = height * (0.12f + row * 0.2f)
                    val diamond = Path().apply {
                        moveTo(x, y - side * 0.095f)
                        lineTo(x + side * 0.095f, y)
                        lineTo(x, y + side * 0.095f)
                        lineTo(x - side * 0.095f, y)
                        close()
                    }
                    drawPath(
                        diamond,
                        if ((row + column + layer) % 2 == 0) secondary else bright
                    )
                }
            }
            drawCircle(white, side * 0.05f, center)
        }
        6 -> {
            for (dot in 0 until 28) {
                val progress = dot / 27f
                val angle = progress * (PI * (3.5 + layer * 0.22)).toFloat()
                val radius = side * (0.04f + progress * 0.52f)
                val point = Offset(
                    center.x + cos(angle.toDouble()).toFloat() * radius,
                    center.y + sin(angle.toDouble()).toFloat() * radius
                )
                drawCircle(if (dot % 3 == 0) bright else secondary, side * (0.018f + progress * 0.014f), point)
            }
        }
        7 -> {
            for (square in 0..4) {
                val inset = side * (0.06f + square * 0.09f)
                rotate((square * 13 + layer * 9).toFloat(), center) {
                    drawRoundRect(
                        if ((square + layer) % 2 == 0) secondary else bright,
                        Offset(inset, inset * 0.7f),
                        Size(width - inset * 2f, height - inset * 1.4f),
                        androidx.compose.ui.geometry.CornerRadius(side * 0.025f),
                        style = Stroke(width = side * (0.016f + layer * 0.003f))
                    )
                }
            }
        }
        8 -> {
            for (arc in 0..4) {
                val inset = side * (-0.18f + arc * 0.08f)
                drawArc(
                    color = if ((arc + layer) % 2 == 0) bright else secondary,
                    startAngle = 28f + layer * 12f + arc * 26f,
                    sweepAngle = 92f + layer * 8f,
                    useCenter = false,
                    topLeft = Offset(inset, height * (0.08f + arc * 0.04f)),
                    size = Size(side * (0.88f + arc * 0.08f), side * (0.88f + arc * 0.08f)),
                    style = Stroke(width = side * (0.025f + arc * 0.008f), cap = StrokeCap.Round)
                )
            }
        }
        else -> {
            val rays = 12 + layer * 2
            for (ray in 0 until rays) {
                val angle = (ray.toFloat() / rays) * (2f * PI.toFloat()) + phase
                val inner = side * (0.12f + (ray % 2) * 0.04f)
                val outer = side * (0.56f - (ray % 3) * 0.035f)
                val start = Offset(
                    center.x + cos(angle.toDouble()).toFloat() * inner,
                    center.y + sin(angle.toDouble()).toFloat() * inner
                )
                val end = Offset(
                    center.x + cos(angle.toDouble()).toFloat() * outer,
                    center.y + sin(angle.toDouble()).toFloat() * outer
                )
                drawLine(if (ray % 3 == 0) bright else secondary, start, end, side * 0.035f, cap = StrokeCap.Round)
            }
            drawCircle(palette.start.copy(alpha = 0.85f), side * 0.13f, center)
            drawCircle(white, side * 0.04f, center)
        }
    }
}
