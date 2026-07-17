// JS <-> ExoPlayer bridge, injected into the WebView as `__KROMA_ANDROID__`.
//
// The web app's ExoEngine calls `load` / `command` (which arrive on a WebView
// worker thread - every player access hops to the main thread), and events are
// pushed back by invoking the page's global `__kromaExoEvent(payload)` (see
// packages/tv .../player/exoEngine.ts for the payload contract).
//
// ExoPlayer demuxes MKV/MP4/TS over plain HTTP Range and decodes through the
// platform (hardware HEVC, AC3/EAC3 passthrough or decode), so `direct` loads
// cost the server nothing; `master=true` loads are the server's stream-copy
// HLS remux (media3-exoplayer-hls picks it up from the mime hint).
package tv.kroma.androidtv

import android.app.Activity
import android.os.Handler
import android.os.Looper
import android.webkit.JavascriptInterface
import android.webkit.WebView
import androidx.media3.common.C
import androidx.media3.common.MediaItem
import androidx.media3.common.MimeTypes
import androidx.media3.common.PlaybackException
import androidx.media3.common.Player
import androidx.media3.common.TrackSelectionOverride
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.ui.PlayerView
import org.json.JSONObject

class ExoBridge(
    activity: Activity,
    private val webView: WebView,
    private val playerView: PlayerView,
) {
    private val main = Handler(Looper.getMainLooper())
    private val player: ExoPlayer = ExoPlayer.Builder(activity).build()

    // Position/buffer heartbeat for the web engine's clock (absolute time is
    // reconstructed there: baseSec + this relative position).
    private val ticker = object : Runnable {
        override fun run() {
            if (player.playbackState != Player.STATE_IDLE) {
                emit(JSONObject().put("t", "time").put("sec", player.currentPosition / 1000.0))
                emit(JSONObject().put("t", "buffered").put("sec", player.bufferedPosition / 1000.0))
            }
            main.postDelayed(this, 500)
        }
    }

    private val listener = object : Player.Listener {
        override fun onPlaybackStateChanged(state: Int) {
            when (state) {
                Player.STATE_READY -> {
                    val dur = player.duration
                    if (dur != C.TIME_UNSET) {
                        emit(JSONObject().put("t", "duration").put("sec", dur / 1000.0))
                    }
                    emit(JSONObject().put("t", "ready"))
                    emit(JSONObject().put("t", "waiting").put("active", false))
                }
                Player.STATE_BUFFERING ->
                    emit(JSONObject().put("t", "waiting").put("active", true))
                Player.STATE_ENDED -> emit(JSONObject().put("t", "ended"))
                else -> {}
            }
        }

        override fun onIsPlayingChanged(isPlaying: Boolean) {
            playerView.keepScreenOn = isPlaying // panel stays awake only while playing
            emit(JSONObject().put("t", "state").put("playing", isPlaying))
        }

        override fun onPlayerError(error: PlaybackException) {
            emit(JSONObject().put("t", "error").put("message", error.errorCodeName))
        }
    }

    init {
        playerView.player = player
        player.addListener(listener)
        main.post(ticker)
    }

    /** Load a URL (replaces the current item). `master` = the HLS remux. */
    @JavascriptInterface
    fun load(url: String, startSec: Double, master: Boolean) {
        main.post {
            val item = MediaItem.Builder()
                .setUri(url)
                .apply { if (master) setMimeType(MimeTypes.APPLICATION_M3U8) }
                .build()
            if (startSec > 0.5) player.setMediaItem(item, (startSec * 1000).toLong())
            else player.setMediaItem(item)
            player.playWhenReady = false // the web hook drives the first play()
            player.prepare()
        }
    }

    /** `{op: 'play'|'pause'|'seek'|'audio'|'stop', value?: number}`. */
    @JavascriptInterface
    fun command(json: String) {
        val cmd = JSONObject(json)
        main.post {
            when (cmd.optString("op")) {
                "play" -> player.play()
                "pause" -> player.pause()
                "seek" -> player.seekTo((cmd.optDouble("value", 0.0) * 1000).toLong())
                "audio" -> selectAudio(cmd.optInt("value", 0))
                "stop" -> {
                    player.stop()
                    player.clearMediaItems()
                }
            }
        }
    }

    /** Select the Nth audio track group (audio-relative index, file order) in
     * place - the picture never stops. */
    private fun selectAudio(index: Int) {
        val groups = player.currentTracks.groups.filter { it.type == C.TRACK_TYPE_AUDIO }
        val group = groups.getOrNull(index) ?: return
        player.trackSelectionParameters = player.trackSelectionParameters
            .buildUpon()
            .setOverrideForType(TrackSelectionOverride(group.mediaTrackGroup, 0))
            .build()
    }

    private fun emit(payload: JSONObject) {
        val js = "window.__kromaExoEvent&&window.__kromaExoEvent($payload)"
        webView.post { webView.evaluateJavascript(js, null) }
    }

    fun pauseForBackground() {
        main.post { player.pause() }
    }

    fun release() {
        main.removeCallbacks(ticker)
        main.post { player.release() }
    }
}
