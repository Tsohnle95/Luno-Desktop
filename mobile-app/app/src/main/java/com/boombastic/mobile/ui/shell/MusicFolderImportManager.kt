package com.boombastic.mobile.ui.shell

import android.content.Context
import android.net.Uri
import android.os.SystemClock
import android.widget.Toast
import com.boombastic.mobile.data.repository.LibraryRepository
import com.boombastic.mobile.data.repository.MusicFolderRepository
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

/**
 * Live state of the desktop-style music-folder import, rendered as a strip
 * in the shell from the Settings-drawer launcher.  `null`
 * means no import is running or being reported.
 */
sealed interface FolderImportStatus {
    data class Importing(
        val imported: Int,
        val duplicates: Int,
        val errors: Int
    ) : FolderImportStatus

    /** The import reached its final state (completed, stalled, or failed).
     *  The strip renders the matching message for a couple of seconds, then
     *  the manager clears itself.  A stalled import is STILL RUNNING (the
     *  watchdog never cancels it — every unit of work is timeout-bounded,
     *  so it will finish and report its real outcome).  [errorMessage]
     *  carries the exception text of a failed import for diagnosis. */
    data class Finished(
        val imported: Int,
        val duplicates: Int,
        val errors: Int,
        val stalled: Boolean,
        val failed: Boolean = false,
        val persistWarning: String? = null,
        val errorMessage: String? = null
    ) : FolderImportStatus
}

/**
 * Process-lifetime owner of the music-folder import: launches the import
 * on [appScope], publishes progress as a thread-safe [StateFlow], and —
 * critically — **always** resolves the strip itself.
 *
 * The old design cleared the strip from the import coroutine's `finally`,
 * which never ran when the import wedged in a non-cancellable provider
 * call, so the "Importing music folder…" strip could sit frozen at the
 * final counts forever.  Here the stall watchdog transitions the strip to
 * its final state directly (and cancels the wedged job as a best effort),
 * so the UI always resolves to "Import complete!" / a stall message within
 * the stall window, then auto-clears after a short delay.
 */
class MusicFolderImportManager(
    private val appScope: CoroutineScope,
    private val libraryRepository: LibraryRepository,
    private val musicFolderRepository: MusicFolderRepository,
    private val context: Context
) {
    private val _status = MutableStateFlow<FolderImportStatus?>(null)
    val status: StateFlow<FolderImportStatus?> = _status.asStateFlow()

    /** The running import job (if any) — lets a retry replace a previous
     *  attempt instead of stacking two imports. */
    private var activeJob: Job? = null

    /** Starts a music-folder import; a retry replaces any previous attempt. */
    fun start(treeUri: Uri) {
        // Best-effort cancel of any previous attempt: a cancellable job is
        // stopped; one wedged in a non-cancellable call simply lingers in
        // the background while the new import takes over (its abandoned
        // threads are bounded, so it cannot exhaust the process).
        activeJob?.cancel()
        _status.value = FolderImportStatus.Importing(0, 0, 0)
        // AtomicLong, not a plain var: the import coroutine writes it from
        // IO/Default threads and the watchdog coroutine reads it — a plain
        // captured field has no memory barrier, so the JIT could hoist the
        // read and the watchdog would keep comparing against the *initial*
        // timestamp, falsely cancelling a perfectly healthy import once the
        // stall window elapsed (~1800 songs in — the "import stalled" bug).
        val lastTickAt = java.util.concurrent.atomic.AtomicLong(SystemClock.elapsedRealtime())

        val importJob: Job = appScope.launch {
            try {
                musicFolderRepository.saveTreeUri(treeUri)
                val result = libraryRepository.importLibraryTree(treeUri) { imported, duplicates, errors ->
                    lastTickAt.set(SystemClock.elapsedRealtime())
                    _status.value = FolderImportStatus.Importing(imported, duplicates, errors)
                }
                val persistWarning = if (result.persistFailures > 0) {
                    " — storage access not persisted (${result.persistFailures}): " +
                        "these songs may not play after a restart"
                } else {
                    ""
                }
                // Main-thread + guarded: a toast failure must never crash
                // the app or flip a completed import into the "failed"
                // state (this coroutine runs on Dispatchers.Default).
                showToast(
                    "Music folder set: ${result.imported} songs imported, " +
                        "${result.duplicates} duplicates, ${result.errors} errors$persistWarning",
                    long = true
                )
                finish(
                    FolderImportStatus.Finished(
                        imported = result.imported,
                        duplicates = result.duplicates,
                        errors = result.errors,
                        stalled = false,
                        failed = false,
                        persistWarning = persistWarning
                    )
                )
            } catch (e: CancellationException) {
                throw e
            } catch (e: Throwable) {
                // Unexpected failure — resolve the strip (never leave a
                // frozen "Importing…" behind), log for diagnosis, and
                // surface the exception message to the user so the cause
                // can be reported without logcat.
                android.util.Log.e("MusicFolderImport", "Folder import failed", e)
                val message = e.message ?: e.javaClass.simpleName
                val current = _status.value as? FolderImportStatus.Importing
                finish(
                    FolderImportStatus.Finished(
                        imported = current?.imported ?: 0,
                        duplicates = current?.duplicates ?: 0,
                        errors = current?.errors ?: 0,
                        stalled = false,
                        failed = true,
                        errorMessage = message
                    )
                )
                showToast("Import failed: $message", long = true)
            }
        }
        activeJob = importJob
        importJob.invokeOnCompletion {
            if (activeJob === importJob) activeJob = null
        }

        // Stall watchdog: independent of the import job.  Every unit of
        // import work is timeout-bounded, so a quiet stretch means the
        // provider/device is slow — NOT that the import is dead.  It
        // therefore NEVER cancels the job (cancelling a slow-but-alive
        // import is what guaranteed failure); it only warns and resolves
        // the strip, and the import keeps running and reports its real
        // outcome ("Import complete!" / "Import failed") when it finishes.
        appScope.launch {
            while (true) {
                delay(STALL_CHECK_INTERVAL_MS)
                if (importJob.isCompleted) return@launch
                if (SystemClock.elapsedRealtime() - lastTickAt.get() > IMPORT_STALL_TIMEOUT_MS) {
                    val current = _status.value as? FolderImportStatus.Importing
                    finish(
                        FolderImportStatus.Finished(
                            imported = current?.imported ?: 0,
                            duplicates = current?.duplicates ?: 0,
                            errors = current?.errors ?: 0,
                            stalled = true,
                            failed = false
                        )
                    )
                    showToast(
                        "Import is taking longer than expected — you can keep using the app",
                        long = true
                    )
                    return@launch
                }
            }
        }
    }

    /** Publishes the final state and auto-clears it after a short display. */
    private fun finish(status: FolderImportStatus.Finished) {
        _status.value = status
        appScope.launch {
            delay(COMPLETE_DISPLAY_MS)
            if (_status.value == status) _status.value = null
        }
    }

    /**
     * Main-thread, guarded toast.  This manager's coroutines run on
     * [appScope] (Dispatchers.Default) — `Toast.makeText` there throws
     * "Can't toast on a thread that has not called Looper.prepare()".
     * The toast is posted to the main looper and wrapped in `runCatching`
     * so a toast failure can never crash the app or flip import state.
     */
    private fun showToast(text: String, long: Boolean = false) {
        runCatching {
            android.os.Handler(android.os.Looper.getMainLooper()).post {
                runCatching {
                    Toast.makeText(
                        context,
                        text,
                        if (long) Toast.LENGTH_LONG else Toast.LENGTH_SHORT
                    ).show()
                }
            }
        }
    }

    companion object {
        /** No progress for this long = the import is slow (it is never
         *  cancelled — it just gets a "taking longer than expected" note).
         *  Every unit of import work is timeout-bounded and folder
         *  traversal ticks progress, so this only fires when the provider
         *  or device is genuinely crawling. */
        private const val IMPORT_STALL_TIMEOUT_MS = 90_000L

        /** How often the stall watchdog checks for progress. */
        private const val STALL_CHECK_INTERVAL_MS = 15_000L

        /** How long "Import complete!" / the stall message stays visible. */
        private const val COMPLETE_DISPLAY_MS = 2_000L
    }
}
