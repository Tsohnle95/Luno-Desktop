package com.boombastic.mobile.ui.discover

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import com.boombastic.mobile.ui.theme.Dimens
import com.boombastic.mobile.ui.theme.PrimaryText
import com.boombastic.mobile.ui.theme.SecondaryText

/**
 * Discover screen — honest non-interactive empty state.
 * No fake recommendations or mocked data.
 * Will show Last.fm-powered recommendations when implemented.
 */
@Composable
fun DiscoverScreen() {
    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(horizontal = Dimens.paddingLarge),
        verticalArrangement = Arrangement.Center,
        horizontalAlignment = Alignment.CenterHorizontally
    ) {
        Text(
            text = "Discover",
            style = androidx.compose.material3.MaterialTheme.typography.headlineLarge,
            color = PrimaryText,
            modifier = Modifier.padding(bottom = Dimens.paddingMedium),
            textAlign = TextAlign.Center
        )

        Text(
            text = "Recommendations from Last.fm will appear here.",
            style = androidx.compose.material3.MaterialTheme.typography.bodyLarge,
            color = SecondaryText,
            textAlign = TextAlign.Center,
            modifier = Modifier.padding(horizontal = Dimens.paddingXLarge)
        )

        Text(
            text = "This feature requires a Last.fm API key and will be available in a future update.",
            style = androidx.compose.material3.MaterialTheme.typography.bodyMedium,
            color = SecondaryText,
            textAlign = TextAlign.Center,
            modifier = Modifier.padding(
                top = Dimens.paddingMedium,
                start = Dimens.paddingXLarge,
                end = Dimens.paddingXLarge
            )
        )
    }
}
