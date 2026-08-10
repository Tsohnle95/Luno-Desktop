package com.luno.mobile.ui.components

import com.google.common.truth.Truth.assertThat
import org.junit.Test

class RecommendationArtworkTest {

    @Test
    fun designIndex_isDeterministicAndWithinTheSeventyFiveDesigns() {
        val first = recommendationArtworkDesignIndex("Artist", "Song")
        val second = recommendationArtworkDesignIndex("artist", "song")

        assertThat(first).isEqualTo(second)
        assertThat(first).isAtLeast(0)
        assertThat(first).isLessThan(RECOMMENDATION_ART_DESIGN_COUNT)
    }

    @Test
    fun equivalentRecommendationText_sharesDesign() {
        assertThat(
            recommendationArtworkDesignIndex("Beyoncé", "Puttin' on the Ritz")
        ).isEqualTo(
            recommendationArtworkDesignIndex("beyonce", "Puttin on the Ritz")
        )
    }

    @Test
    fun differentRecommendations_canReceiveDifferentDesigns() {
        val designs = (0 until 40)
            .map { index -> recommendationArtworkDesignIndex("Artist $index", "Song $index") }
            .toSet()

        assertThat(designs.size).isGreaterThan(1)
    }
}
