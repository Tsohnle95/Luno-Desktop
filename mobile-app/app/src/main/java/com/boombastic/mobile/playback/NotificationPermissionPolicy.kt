package com.boombastic.mobile.playback

import android.Manifest
import android.content.Context
import android.content.SharedPreferences
import android.content.pm.PackageManager
import android.os.Build
import androidx.core.content.ContextCompat

/**
 * One-shot notification permission prompt policy.
 *
 * - API < 33: never prompts (POST_NOTIFICATIONS does not exist).
 * - API 33+, already granted: never prompts.
 * - API 33+, ungranted and never automatically attempted: prompts once and records
 *   the attempt. Denial/dismissal does not trigger repeated automatic prompts.
 * - Later grant via Settings is detected naturally via [isGranted].
 * - Playback dispatch is always independent of the prompt result. Per Android
 *   documentation (see
 *   [Notification runtime permission exemptions](https://developer.android.com/develop/ui/views/notifications/notification-permission#exemptions-media-sessions)),
 *   media-session notifications are exempt from POST_NOTIFICATIONS, so a denied
 *   permission does NOT block `startForeground()` for a mediaPlayback FGS.
 *
 * This class contains no Compose dependencies and is fully unit-testable.
 */
class NotificationPermissionPolicy(private val prefs: SharedPreferences) {

    /**
     * Returns `true` if the system permission dialog should be shown now.
     * Callers should invoke [recordPromptAttempted] immediately before launching
     * the platform request.
     */
    fun shouldPrompt(sdkInt: Int = Build.VERSION.SDK_INT, isGranted: Boolean): Boolean {
        if (sdkInt < Build.VERSION_CODES.TIRAMISU) return false
        if (isGranted) return false
        if (wasPromptAttempted()) return false
        return true
    }

    /** Returns whether the one-shot automatic prompt has already been attempted. */
    fun wasPromptAttempted(): Boolean =
        prefs.getBoolean(KEY_PROMPT_ATTEMPTED, false)

    /** Persistently record that the automatic prompt was launched.
     * Uses [commit] for durability; this is a one-shot boolean so the
     * synchronous disk write is acceptable and ensures the flag survives
     * process death immediately. */
    fun recordPromptAttempted() {
        prefs.edit().putBoolean(KEY_PROMPT_ATTEMPTED, true).commit()
    }

    /** Convenience — checks actual OS grant state for POST_NOTIFICATIONS. */
    fun isGranted(context: Context): Boolean {
        return ContextCompat.checkSelfPermission(
            context,
            Manifest.permission.POST_NOTIFICATIONS
        ) == PackageManager.PERMISSION_GRANTED
    }

    companion object {
        private const val KEY_PROMPT_ATTEMPTED = "notification_prompt_attempted"

        private const val PREFS_NAME = "boombastic_playback_prefs"

        /** Factory that uses the app-wide shared preferences. */
        fun create(context: Context): NotificationPermissionPolicy {
            val prefs = context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
            return NotificationPermissionPolicy(prefs)
        }
    }
}
