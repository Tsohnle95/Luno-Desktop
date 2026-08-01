package com.boombastic.mobile.data.repository

import android.content.Context
import android.net.Uri

/**
 * Persists the user's chosen music folder (SAF tree URI) so the folder
 * destination survives app restarts and can be re-imported / changed from
 * the Settings drawer.
 */
class MusicFolderRepository(context: Context) {

    private val prefs = context.applicationContext
        .getSharedPreferences("music_folder", Context.MODE_PRIVATE)

    fun saveTreeUri(uri: Uri) {
        prefs.edit().putString(KEY_TREE_URI, uri.toString()).apply()
    }

    fun loadTreeUri(): String? = prefs.getString(KEY_TREE_URI, null)

    companion object {
        private const val KEY_TREE_URI = "tree_uri"
    }
}
