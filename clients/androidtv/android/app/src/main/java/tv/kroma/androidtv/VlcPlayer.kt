// libVLC software-decode player: the fallback for anything ExoPlayer can't decode
// on the device's platform decoders (10-bit HEVC Main10, DTS, TrueHD, ...). libVLC
// carries its own ffmpeg-based decoders, so it plays those in software.
//
// Renders into its own SurfaceView (behind the transparent WebView, like the
// ExoPlayer plane) and emits the SAME event shape ExoBridge does
// (`{t:'time'|'duration'|'state'|'waiting'|'ready'|'ended'|'error'}`), so the web
// engine drives it through the existing contract without knowing which player is live.
package tv.kroma.androidtv

import android.content.Context
import android.net.Uri
import android.view.SurfaceHolder
import android.view.SurfaceView
import android.view.View
import org.json.JSONObject
import org.videolan.libvlc.LibVLC
import org.videolan.libvlc.Media
import org.videolan.libvlc.MediaPlayer

class VlcPlayer(
    context: Context,
    private val surface: SurfaceView,
    private val onEvent: (JSONObject) -> Unit,
) {
    // --no-drop-late-frames / --no-skip-frames: software HEVC on a weak box is slow;
    // keep every frame rather than stutter-skip. network-caching smooths HTTP range.
    private val libVlc = LibVLC(
        context,
        arrayListOf("--no-drop-late-frames", "--no-skip-frames", "--network-caching=1500"),
    )
    private val player = MediaPlayer(libVlc)
    private var attached = false
    private var reportedReady = false

    init {
        player.setEventListener { ev -> handle(ev) }
        // The surface is GONE until VLC activates, so its size is unknown when we
        // first attach; feed VLC the REAL dimensions once the surface is laid out
        // (else it renders into a wrong-sized corner of the plane).
        surface.holder.addCallback(object : SurfaceHolder.Callback {
            override fun surfaceCreated(holder: SurfaceHolder) {}
            override fun surfaceChanged(holder: SurfaceHolder, f: Int, w: Int, h: Int) {
                if (attached && w > 0 && h > 0) player.vlcVout.setWindowSize(w, h)
            }
            override fun surfaceDestroyed(holder: SurfaceHolder) {}
        })
    }

    /** Load a URL and start playing at `startSec` (the fallback takes over mid-play,
     * so it auto-plays rather than waiting for a play command). */
    fun load(url: String, startSec: Double) {
        reportedReady = false
        surface.visibility = View.VISIBLE
        ensureAttached()
        val media = Media(libVlc, Uri.parse(url)).apply {
            setHWDecoderEnabled(true, false) // try HW, fall back to SW automatically
        }
        player.media = media
        media.release()
        if (startSec > 0.5) player.time = (startSec * 1000).toLong()
        player.play()
    }

    fun play() = player.play()
    fun pause() = player.pause()
    fun seek(sec: Double) { player.time = (sec * 1000).toLong().coerceAtLeast(0) }

    /** Select an audio track by its ZERO-BASED order among audio tracks (the web
     * engine speaks audio-relative indices; VLC ids are absolute, so map them). */
    fun setAudio(index: Int) {
        val ids = player.audioTracks?.map { it.id } ?: return
        ids.getOrNull(index)?.let { player.audioTrack = it }
    }

    fun stop() {
        player.stop()
        surface.visibility = View.GONE
        if (attached) {
            player.vlcVout.detachViews()
            attached = false
        }
    }

    fun release() {
        stop()
        player.release()
        libVlc.release()
    }

    private fun ensureAttached() {
        if (attached) return
        val vout = player.vlcVout
        vout.setVideoView(surface)
        val w = surface.width.takeIf { it > 0 } ?: 1920
        val h = surface.height.takeIf { it > 0 } ?: 1080
        vout.setWindowSize(w, h)
        vout.attachViews()
        attached = true
    }

    private fun handle(ev: MediaPlayer.Event) {
        when (ev.type) {
            MediaPlayer.Event.Opening, MediaPlayer.Event.Buffering ->
                emit("waiting", "active", ev.type == MediaPlayer.Event.Opening || ev.buffering < 100f)
            MediaPlayer.Event.Playing -> {
                announceReady()
                emit("state", "playing", true)
                emit("waiting", "active", false)
            }
            MediaPlayer.Event.Paused -> emit("state", "playing", false)
            MediaPlayer.Event.TimeChanged ->
                onEvent(JSONObject().put("t", "time").put("sec", ev.timeChanged / 1000.0))
            MediaPlayer.Event.LengthChanged -> {
                val len = ev.lengthChanged
                if (len > 0) onEvent(JSONObject().put("t", "duration").put("sec", len / 1000.0))
            }
            MediaPlayer.Event.EndReached -> onEvent(JSONObject().put("t", "ended"))
            MediaPlayer.Event.EncounteredError ->
                // VLC couldn't play it either: a real error (audio=false, no more fallbacks).
                onEvent(JSONObject().put("t", "error").put("message", "VLC_ERROR").put("audio", false))
        }
    }

    /** Fire `ready` once so the web engine runs its onLoaded (audio track + resume). */
    private fun announceReady() {
        if (reportedReady) return
        reportedReady = true
        val len = player.length
        if (len > 0) onEvent(JSONObject().put("t", "duration").put("sec", len / 1000.0))
        onEvent(JSONObject().put("t", "ready"))
    }

    private fun emit(t: String, key: String, value: Boolean) {
        onEvent(JSONObject().put("t", t).put(key, value))
    }
}
