package com.luno.mobile.data.discovery

import android.content.Context
import android.content.SharedPreferences
import android.util.Log
import androidx.security.crypto.EncryptedSharedPreferences
import androidx.security.crypto.MasterKey

/**
 * Encrypted storage for the user's Last.fm API key
 * (EncryptedSharedPreferences, AES256-GCM values / AES256-SIV keys).
 *
 * The key is never logged, never exported in JSON manifests, and only ever
 * sent to ws.audioscrobbler.com over HTTPS (see [LastfmService]).
 *
 * If the Android Keystore is unavailable (corrupted keystore, exotic
 * device), construction falls back to plain SharedPreferences with a
 * warning — a crash on key-storage failure is worse than the downgrade.
 */
class LastfmKeyStore(context: Context) {

    companion object {
        private const val TAG = "LastfmKeyStore"
        private const val PREFS_NAME = "lastfm_secure_prefs"
        private const val KEY_API_KEY = "lastfm_api_key"
    }

    private val prefs: SharedPreferences = runCatching {
        val masterKey = MasterKey.Builder(context)
            .setKeyScheme(MasterKey.KeyScheme.AES256_GCM)
            .build()
        EncryptedSharedPreferences.create(
            context,
            PREFS_NAME,
            masterKey,
            EncryptedSharedPreferences.PrefKeyEncryptionScheme.AES256_SIV,
            EncryptedSharedPreferences.PrefValueEncryptionScheme.AES256_GCM
        )
    }.getOrElse { e ->
        Log.w(TAG, "EncryptedSharedPreferences unavailable (${e.message}); using plain prefs")
        context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
    }

    fun getKey(): String? = prefs.getString(KEY_API_KEY, null)?.takeIf { it.isNotBlank() }

    fun setKey(key: String) {
        prefs.edit().putString(KEY_API_KEY, key.trim()).apply()
    }

    fun clearKey() {
        prefs.edit().remove(KEY_API_KEY).apply()
    }
}
