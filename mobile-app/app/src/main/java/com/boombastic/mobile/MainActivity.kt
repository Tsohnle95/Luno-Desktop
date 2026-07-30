package com.boombastic.mobile

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.annotation.VisibleForTesting
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.Surface
import androidx.compose.ui.Modifier
import com.boombastic.mobile.playback.MusicController
import com.boombastic.mobile.ui.shell.MainShell
import com.boombastic.mobile.ui.theme.BoomBasticTheme
import com.boombastic.mobile.ui.theme.PrimaryBackground

class MainActivity : ComponentActivity() {

    @VisibleForTesting
    internal lateinit var musicController: MusicController

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()

        musicController = MusicController(this)
        musicController.initialize()

        setContent {
            BoomBasticTheme {
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
        musicController.release()
        super.onDestroy()
    }
}
