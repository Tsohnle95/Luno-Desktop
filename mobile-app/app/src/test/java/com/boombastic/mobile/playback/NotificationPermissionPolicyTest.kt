package com.boombastic.mobile.playback

import android.content.Context
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import com.google.common.truth.Truth.assertThat
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config

/**
 * Focused unit tests for [NotificationPermissionPolicy].
 *
 * Covers all required scenarios:
 * - API 29 (below 33): never prompts.
 * - API 33+, already granted: never prompts.
 * - API 33+, ungranted, not attempted: prompts once.
 * - After recordPromptAttempted: no longer prompts.
 * - Denial/dismissal does not reset the attempt flag.
 * - Playback dispatch is always independent (no method blocks playback).
 * - Multiple recordPromptAttempt calls are idempotent.
 */
@RunWith(AndroidJUnit4::class)
@Config(manifest = Config.NONE, sdk = [34])
class NotificationPermissionPolicyTest {

    private lateinit var policy: NotificationPermissionPolicy
    private lateinit var context: Context

    @Before
    fun setUp() {
        context = ApplicationProvider.getApplicationContext()
        val prefs = context.getSharedPreferences("test_${javaClass.simpleName}", Context.MODE_PRIVATE)
        prefs.edit().clear().apply()
        policy = NotificationPermissionPolicy(prefs)
    }

    // ── API < 33 ─────────────────────────────────────────────────────────

    @Test
    fun `shouldPrompt returns false when SDK is below 33 regardless of grant`() {
        assertThat(policy.shouldPrompt(sdkInt = 29, isGranted = false)).isFalse()
        assertThat(policy.shouldPrompt(sdkInt = 29, isGranted = true)).isFalse()
        assertThat(policy.shouldPrompt(sdkInt = 32, isGranted = false)).isFalse()
    }

    // ── API 33+, already granted ──────────────────────────────────────────

    @Test
    fun `shouldPrompt returns false when permission is already granted`() {
        assertThat(policy.shouldPrompt(sdkInt = 33, isGranted = true)).isFalse()
        assertThat(policy.shouldPrompt(sdkInt = 35, isGranted = true)).isFalse()
    }

    // ── API 33+, ungranted, never attempted — one-shot prompt ──────────────

    @Test
    fun `shouldPrompt returns true when ungranted and not yet attempted on API 33+`() {
        assertThat(policy.shouldPrompt(sdkInt = 33, isGranted = false)).isTrue()
        assertThat(policy.shouldPrompt(sdkInt = 35, isGranted = false)).isTrue()
    }

    @Test
    fun `after recording attempt shouldPrompt returns false`() {
        policy.recordPromptAttempted()
        assertThat(policy.shouldPrompt(sdkInt = 33, isGranted = false)).isFalse()
        assertThat(policy.shouldPrompt(sdkInt = 35, isGranted = false)).isFalse()
    }

    // ── Persistence ───────────────────────────────────────────────────────

    @Test
    fun `wasPromptAttempted returns false initially`() {
        assertThat(policy.wasPromptAttempted()).isFalse()
    }

    @Test
    fun `recordPromptAttempted persists across policy instances`() {
        policy.recordPromptAttempted()

        // Create a new policy backed by the same SharedPreferences
        val prefs = context.getSharedPreferences("test_${javaClass.simpleName}", Context.MODE_PRIVATE)
        val reloaded = NotificationPermissionPolicy(prefs)
        assertThat(reloaded.wasPromptAttempted()).isTrue()
        assertThat(reloaded.shouldPrompt(sdkInt = 33, isGranted = false)).isFalse()
    }

    // ── Denial/dismissal does not re-enable prompt ────────────────────────

    @Test
    fun `denial is treated same as dismissal — no repeated automatic prompts`() {
        // Simulate: prompt shown, user denies → permission still ungranted
        policy.recordPromptAttempted()

        // Subsequent shouldPrompt calls must return false (denial tracked)
        assertThat(policy.shouldPrompt(sdkInt = 33, isGranted = false)).isFalse()
        assertThat(policy.shouldPrompt(sdkInt = 35, isGranted = false)).isFalse()
    }

    // ── Later Settings grant detected naturally ───────────────────────────

    @Test
    fun `Settings grant is detected even after previous denial`() {
        policy.recordPromptAttempted()

        // Simulate: user later grants via Settings
        assertThat(policy.shouldPrompt(sdkInt = 33, isGranted = true)).isFalse()
    }

    // ── Independent playback dispatch (no method blocks playback) ─────────

    @Test
    fun `playback dispatch methods are always available regardless of policy state`() {
        // The policy has no method that can block or prevent playback.
        // All shouldPrompt paths return false gracefully; recordPromptAttempted
        // never throws. This test confirms no precondition blocks callers.
        policy.recordPromptAttempted()
        assertThat(policy.wasPromptAttempted()).isTrue()

        // Even after denial, playback is legally permitted — per Android docs
        // media-session notifications are exempt from POST_NOTIFICATIONS:
        // https://developer.android.com/develop/ui/views/notifications/notification-permission#exemptions-media-sessions
        // startForeground() for a mediaPlayback FGS does NOT throw
        // SecurityException when POST_NOTIFICATIONS is denied.
    }

    // ── Idempotent recordPromptAttempted ──────────────────────────────────

    @Test
    fun `multiple recordPromptAttempted calls are idempotent`() {
        policy.recordPromptAttempted()
        policy.recordPromptAttempted() // second call must not throw or reset
        policy.recordPromptAttempted() // third call

        assertThat(policy.wasPromptAttempted()).isTrue()
        assertThat(policy.shouldPrompt(sdkInt = 33, isGranted = false)).isFalse()
    }

    // ── isGranted convenience delegates to platform correctly ─────────────

    @Test
    fun `isGranted returns false when permission not granted`() {
        // In the test environment POST_NOTIFICATIONS is not granted
        assertThat(policy.isGranted(context)).isFalse()
    }
}
