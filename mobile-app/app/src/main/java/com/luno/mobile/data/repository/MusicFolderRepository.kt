package com.luno.mobile.data.repository

import android.content.Context
import android.net.Uri
import android.provider.DocumentsContract
import java.io.File
import java.io.IOException

/**
 * Persists the user's chosen music folder (SAF tree URI) so the folder
 * destination survives app restarts and can be re-imported / changed from
 * the Settings drawer.
 */
class MusicFolderRepository(context: Context) {

    private val context = context.applicationContext
    private val prefs = context
        .getSharedPreferences("music_folder", Context.MODE_PRIVATE)

    fun saveTreeUri(uri: Uri) {
        prefs.edit().putString(KEY_TREE_URI, uri.toString()).apply()
    }

    fun loadTreeUri(): String? = prefs.getString(KEY_TREE_URI, null)

    /** Copies a local file into the selected SAF folder and returns its URI. */
    fun copyFileToSelectedFolder(
        sourceFile: File,
        displayName: String,
        mimeType: String
    ): Uri? {
        val savedTreeUri = loadTreeUri()?.takeUnless { it.isBlank() } ?: return null
        val treeUri = Uri.parse(savedTreeUri)
        val rootDocumentId = try {
            DocumentsContract.getTreeDocumentId(treeUri)
        } catch (e: Exception) {
            throw IOException("The selected music folder is no longer available", e)
        }
        val parentUri = try {
            DocumentsContract.buildDocumentUriUsingTree(
                treeUri,
                rootDocumentId
            )
        } catch (e: Exception) {
            throw IOException("The selected music folder is no longer available", e)
        }

        // Do not create a second document when the selected folder already
        // contains this downloaded filename. Reuse its URI so the library
        // points at the existing destination file.
        findChildByName(treeUri, rootDocumentId, displayName)?.let { return it }

        var destinationUri: Uri? = null
        try {
            destinationUri = DocumentsContract.createDocument(
                context.contentResolver,
                parentUri,
                mimeType,
                displayName
            ) ?: throw IOException("Could not create a file in the selected music folder")

            context.contentResolver.openOutputStream(destinationUri, "w")?.use { output ->
                sourceFile.inputStream().use { input -> input.copyTo(output) }
            } ?: throw IOException("Could not write to the selected music folder")

            return destinationUri
        } catch (e: Exception) {
            destinationUri?.let { created ->
                runCatching { DocumentsContract.deleteDocument(context.contentResolver, created) }
            }
            if (e is IOException) throw e
            throw IOException("Could not save the song to the selected music folder", e)
        }
    }

    private fun findChildByName(
        treeUri: Uri,
        parentDocumentId: String,
        displayName: String
    ): Uri? {
        val childrenUri = DocumentsContract.buildChildDocumentsUriUsingTree(
            treeUri,
            parentDocumentId
        )
        return try {
            context.contentResolver.query(
                childrenUri,
                arrayOf(
                    DocumentsContract.Document.COLUMN_DOCUMENT_ID,
                    DocumentsContract.Document.COLUMN_DISPLAY_NAME,
                    DocumentsContract.Document.COLUMN_MIME_TYPE
                ),
                null,
                null,
                null
            )?.use { cursor ->
                val idIndex = cursor.getColumnIndex(DocumentsContract.Document.COLUMN_DOCUMENT_ID)
                val nameIndex = cursor.getColumnIndex(DocumentsContract.Document.COLUMN_DISPLAY_NAME)
                val mimeIndex = cursor.getColumnIndex(DocumentsContract.Document.COLUMN_MIME_TYPE)
                while (cursor.moveToNext()) {
                    if (idIndex >= 0 && nameIndex >= 0 &&
                        cursor.getString(nameIndex) == displayName &&
                        (mimeIndex < 0 || cursor.getString(mimeIndex) != DocumentsContract.Document.MIME_TYPE_DIR)
                    ) {
                        return@use DocumentsContract.buildDocumentUriUsingTree(
                            treeUri,
                            cursor.getString(idIndex)
                        )
                    }
                }
                null
            }
        } catch (e: Exception) {
            throw IOException("Could not inspect the selected music folder", e)
        }
    }

    companion object {
        private const val KEY_TREE_URI = "tree_uri"
    }
}
