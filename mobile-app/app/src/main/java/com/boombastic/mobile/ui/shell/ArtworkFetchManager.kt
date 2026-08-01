package com.boombastic.mobile.ui.shell

import android.content.Context
import android.util.Log
import android.widget.Toast
import com.boombastic.mobile.data.repository.LibraryRepository
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

/**
 * Live state of the "Fetch missing artwork" sweep, rendered as a strip in
 * the shell (Settings drawer).  `null` means nothing is running or being
 * reported.
 */
sealed interface ArtworkFetchStatus {
    data class Progress(
        val scanned: Int,
        val total: Int,
        val updated: Int
    ) : ArtworkFetchStatus

    /** The sweep reached its final state; the strip shows the message for
     *  a couple of seconds, then the manager clears itself. */
    data class Finished(
        val updated: Int,
        val failed: Boolean = false,
        val errorMessage: String? = null
    ) : ArtworkFetchStatus
}

/**
 * Process-lifetime owner of the missing-artwork sweep (desktop "Fetch
 * missing artwork" parity): launches the scan on [appScope], publishes
 * per-track progress as a thread-safe [StateFlow] so the shell can render
 * a live "Fetching missing artwork… N/M" strip, and always resolves the
 * strip itself ("Artwork fetched for N track(s)" / "No missing artwork
 * found" / failure) before auto-clearing.
 *
 * The old inline implementation closed the drawer and showed nothing until
 * the completion toast — over a 4000-track library the sweep runs for
 * minutes with zero feedback.
 */
class ArtworkFetchManager(
    private val appScope: CoroutineScope,
    private val libraryRepository: LibraryRepository,
    private val context: Context,
    private val fetchArtwork: suspend (onProgress: (Int, Int, Int) -> Unit) -> Int =
        { onProgress -> libraryRepository.fetchMissingArtwork(onProgress) }
) {
    private val _status = MutableStateFlow<ArtworkFetchStatus?>(null)
    val status: StateFlow<ArtworkFetchStatus?> = _status.asStateFlow()

    /** The running sweep job (if any) — `start()` is a no-op while active. */
    private var activeJob: Job? = null

    /** Starts the sweep; ignored while one is already running. */
    fun start() {
        if (activeJob?.isActive == true) return
        _status.value = ArtworkFetchStatus.Progress(0, 0, 0)
        val job: Job = appScope.launch {
            try {
                val updated = fetchArtwork { scanned, total, found ->
                    _status.value = ArtworkFetchStatus.Progress(scanned, total, found)
                }
                finish(ArtworkFetchStatus.Finished(updated))
                showToast(
                    if (updated > 0) {
                        "Artwork fetched for $updated track(s)"
                    } else {
                        "No missing artwork found"
                    }
                )
            } catch (e: CancellationException) {
                throw e
            } catch (e: Throwable) {
                // Never leave a frozen "Fetching…" strip behind; surface
                // the cause without logcat (desktop error-log parity).
                Log.e("ArtworkFetch", "Missing-artwork sweep failed", e)
                val message = e.message ?: e.javaClass.simpleName
                finish(ArtworkFetchStatus.Finished(updated = 0, failed = true, errorMessage = message))
                showToast("Artwork fetch failed: $message", long = true)
            }
        }
        activeJob = job
        job.invokeOnCompletion {
            if (activeJob === job) activeJob = null
        }
    }

    /** Publishes the final state and auto-clears it after a short display. */
    private fun finish(status: ArtworkFetchStatus.Finished) {
        _status.value = status
        appScope.launch {
            delay(COMPLETE_DISPLAY_MS)
            if (_status.value == status) _status.value = null
        }
    }

    /**
     * Main-thread, guarded toast (appScope runs on Dispatchers.Default —
     * `Toast.makeText` there throws "Can't toast on a thread that has not
     * called Looper.prepare()").
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
        /** How long the final strip message stays visible (long enough to
         *  actually read the count — a quick sweep would otherwise flash
         *  "Artwork fetched for 3 track(s)" and vanish). */
        private const val COMPLETE_DISPLAY_MS = 5_000L
    }
}
