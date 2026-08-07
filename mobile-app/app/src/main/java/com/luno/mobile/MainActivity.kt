package com.luno.mobile

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.annotation.VisibleForTesting
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.Surface
import androidx.compose.ui.Modifier
import com.luno.mobile.playback.MusicController
import com.luno.mobile.ui.shell.MainShell
import com.luno.mobile.ui.theme.LunoTheme
import com.luno.mobile.ui.theme.PrimaryBackground
import kotlinx.coroutines.launch

class MainActivity : ComponentActivity() {

    @VisibleForTesting
    internal lateinit var musicController: MusicController

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()

        val app = application as LunoApp
        musicController = MusicController(
            this,
            onTrackPlayed = { uri, playlistId ->
                app.appScope.launch {
                    app.libraryRepository.recordPlayback(uri, playlistId)
                }
            }
        )
        app.recommendationPreviewManager.attach(musicController)
        musicController.initialize()

        setContent {
            LunoTheme {
                Surface(
                    modifier = Modifier.fillMaxSize(),
                    color = PrimaryBackground
                ) {
                    MainShell(musicController = musicController)
                }
            }
        }
    }

    override fun onDestroy() {
        (application as LunoApp).recommendationPreviewManager.detach(musicController)
        musicController.release()
        super.onDestroy()
    }
}
