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
import android.media.audiofx.DynamicsProcessing
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.util.Log
import android.view.SurfaceView
import android.view.View
import android.webkit.JavascriptInterface
import android.webkit.WebView
import androidx.media3.common.C
import androidx.media3.common.Format
import androidx.media3.common.MediaItem
import androidx.media3.common.MimeTypes
import androidx.media3.common.PlaybackException
import androidx.media3.common.Player
import androidx.media3.common.TrackSelectionOverride
import androidx.media3.exoplayer.DecoderReuseEvaluation
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.exoplayer.analytics.AnalyticsListener
import androidx.media3.ui.PlayerView
import org.json.JSONObject

/** logcat tag: everything the bridge logs is a best-effort path, never fatal. */
private const val TAG = "KromaExo"

class ExoBridge(
    private val activity: Activity,
    private val webView: WebView,
    private val playerView: PlayerView,
    private val vlcSurface: SurfaceView,
) {
    private val main = Handler(Looper.getMainLooper())
    private val player: ExoPlayer = ExoPlayer.Builder(activity).build()

    // libVLC software-decode fallback (lazy: only built the first time ExoPlayer
    // hits a codec the device can't decode). `vlcActive` routes commands to it.
    private var vlc: VlcPlayer? = null
    private var vlcActive = false
    private var currentUrl: String? = null

    // When true, EVERY item plays through libVLC from the start (the "libVLC"
    // engine the user can force in Settings), not only as an ExoPlayer
    // decode-failure fallback. Set by setEngine() before the first load().
    private var forceVlc = false

    // Audio filter / volume normalizer: a DynamicsProcessing compressor+limiter on
    // the player's audio session (levels: 0 off, 1 standard, 2 night), re-attached
    // whenever the session id changes. See applyFilter for the tunings.
    private var dynamics: DynamicsProcessing? = null
    private var filterLevel = 0

    // Channel count the live effect is shaped for: an in-place audio track switch
    // (stereo <-> 5.1) needs a new config, the old one would leave the extra
    // channels unprocessed.
    private var filterChannels = 0

    // Can the filter actually change the sound here? API 28+ is the floor; a real
    // attempt then confirms or denies it (see applyFilter / audioFilterSupported).
    // Written on the main thread, read from the WebView's JS thread.
    @Volatile
    private var filterSupported = Build.VERSION.SDK_INT >= Build.VERSION_CODES.P

    // Position/buffer heartbeat for the web engine's clock (absolute time is
    // reconstructed there: baseSec + this relative position).
    // Explicit type required: the lambda references `ticker` (self-reschedule),
    // which makes inferring its type recursive.
    private val ticker: Runnable = Runnable {
        if (player.playbackState != Player.STATE_IDLE) {
            emit(JSONObject().put("t", "time").put("sec", player.currentPosition / 1000.0))
            emit(JSONObject().put("t", "buffered").put("sec", player.bufferedPosition / 1000.0))
        }
        main.postDelayed(ticker, 500)
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
            // Anything the device's hardware/platform decoders can't handle - VIDEO
            // (10-bit HEVC Main10, ...) OR AUDIO (E-AC3/DTS/TrueHD) - hands off to
            // libVLC, which software-decodes any codec with full surround. This is
            // the "plays any file" path; it's transparent to the web engine (VLC
            // emits the same events). Only a non-decode failure (demux, network)
            // falls through to the web engine's own retry.
            if (!vlcActive && (videoUnsupported() || audioUnsupported())) {
                switchToVlc()
                return
            }
            emit(JSONObject().put("t", "error").put("message", error.errorCodeName).put("audio", false))
        }

        override fun onAudioSessionIdChanged(audioSessionId: Int) {
            // A new prepare can rotate the session id, orphaning the attached
            // effect - re-anchor the active filter onto the fresh session.
            if (filterLevel > 0) applyFilter(filterLevel)
        }
    }

    // The decoded audio format is not on Player.Listener, so the effect learns its
    // channel layout here (and re-shapes itself when an in-place track switch
    // changes it).
    private val analytics = object : AnalyticsListener {
        override fun onAudioInputFormatChanged(
            eventTime: AnalyticsListener.EventTime,
            format: Format,
            decoderReuseEvaluation: DecoderReuseEvaluation?,
        ) {
            if (filterLevel > 0 && format.channelCount != filterChannels) applyFilter(filterLevel)
        }
    }

    init {
        playerView.player = player
        player.addListener(listener)
        player.addAnalyticsListener(analytics)
        // Never let ExoPlayer render subtitles: KROMA draws them in the React overlay
        // (fetched + styled + synced by the web player). A selected text track would
        // paint a second, unstyled copy in the PlayerView's SubtitleView.
        player.trackSelectionParameters = player.trackSelectionParameters
            .buildUpon()
            .setTrackTypeDisabled(C.TRACK_TYPE_TEXT, true)
            .build()
        main.post(ticker)
    }

    /** Load a URL (replaces the current item). `master` = the HLS remux. */
    @JavascriptInterface
    fun load(url: String, startSec: Double, master: Boolean) {
        main.post {
            currentUrl = url
            // The forced "libVLC" engine: software-decode this item from the start.
            if (forceVlc) {
                loadVlc(url, startSec)
                return@post
            }
            // A new item always tries ExoPlayer (hardware) first; drop any VLC
            // fallback from the previous item.
            if (vlcActive) {
                vlc?.stop()
                vlcActive = false
                playerView.visibility = View.VISIBLE
            }
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

    /** True when the current media has a VIDEO track the device can't decode and
     * none it can (10-bit HEVC Main10, etc.) - the signal to hand off to libVLC.
     * Main thread only (onPlayerError runs there). */
    private fun videoUnsupported(): Boolean {
        var sawVideo = false
        for (group in player.currentTracks.groups) {
            if (group.type != C.TRACK_TYPE_VIDEO) continue
            sawVideo = true
            for (i in 0 until group.length) if (group.isTrackSupported(i)) return false
        }
        return sawVideo
    }

    /** Stop ExoPlayer and resume the current URL in libVLC (software decode) at the
     * same position - the web engine keeps driving through the same event contract. */
    private fun switchToVlc() {
        val url = currentUrl ?: return
        val posSec = player.currentPosition / 1000.0
        player.stop()
        loadVlc(url, posSec)
    }

    /** Bring up the libVLC plane and (software-)decode `url` from `posSec`, hiding
     * the ExoPlayer surface. Shared by the decode-failure fallback (switchToVlc)
     * and the forced "libVLC" engine (load with forceVlc). */
    private fun loadVlc(url: String, posSec: Double) {
        playerView.visibility = View.GONE
        vlcActive = true
        val v = vlc ?: VlcPlayer(activity, vlcSurface) { emit(it) }.also { vlc = it }
        v.load(url, posSec)
    }

    /** `{op: 'play'|'pause'|'seek'|'audio'|'filter'|'stop'|'rect', value?, x?,y?,w?,h?}`. */
    @JavascriptInterface
    fun command(json: String) {
        val cmd = JSONObject(json)
        main.post {
            val op = cmd.optString("op")
            // When libVLC is driving, route transport (and `rect`, so the video
            // shrinks into the settings card like the ExoPlayer plane does) to it.
            // `filter` (a DynamicsProcessing effect on the Exo audio session) has no
            // VLC equivalent, so it no-ops there.
            val v = vlc
            if (vlcActive && v != null) {
                when (op) {
                    "play" -> v.play()
                    "pause" -> v.pause()
                    "seek" -> v.seek(cmd.optDouble("value", 0.0))
                    "audio" -> v.setAudio(cmd.optInt("value", 0))
                    "rect" -> setRect(cmd)
                    "stop" -> {
                        v.stop()
                        vlcActive = false
                        playerView.visibility = View.VISIBLE
                    }
                }
                return@post
            }
            when (op) {
                "play" -> player.play()
                "pause" -> player.pause()
                "seek" -> player.seekTo((cmd.optDouble("value", 0.0) * 1000).toLong())
                "audio" -> selectAudio(cmd.optInt("value", 0))
                "filter" -> applyFilter(cmd.optInt("value", 0))
                "rect" -> setRect(cmd)
                "stop" -> {
                    player.stop()
                    player.clearMediaItems()
                }
            }
        }
    }

    /** Whether `{op:'filter'}` can actually change the sound on this device, so the
     * page can hide "Filtres audio" instead of showing a control that no-ops: the
     * effect needs API 28+ (minSdk is 21), a decoded audio session (passthrough to
     * an AVR has none) and the platform effect to construct at all. Safe to call
     * before playback - it answers from the last known state (the API floor until
     * a real attempt says otherwise) and never touches the player, which only the
     * main thread may do. A `filterSupported` event follows if the answer flips. */
    @JavascriptInterface
    fun audioFilterSupported(): Boolean = filterSupported

    /** Terminate the whole app (the "Quitter" menu row in the TV shell). Android
     * TV runs a single fullscreen activity with no window chrome, so - like the
     * desktop shell's `app_quit` - the UI must offer the way out. Removes the
     * task from Recents for a clean exit; onDestroy then releases the player. */
    @JavascriptInterface
    fun quit() {
        main.post { activity.finishAndRemoveTask() }
    }

    /** Force the playback engine for subsequent loads: "vlc" makes libVLC the
     * primary player (software-decode EVERYTHING, not just codecs ExoPlayer can't
     * handle); any other value restores the default ExoPlayer-first path. The web
     * ExoEngine calls this at construction when the user picks the "libVLC" engine
     * in Settings. Optional on the bridge: an older APK simply ignores it. */
    @JavascriptInterface
    fun setEngine(mode: String) {
        main.post { forceVlc = mode == "vlc" }
    }

    /** True when the current media carries an audio track this device cannot
     * decode and NONE it can (e.g. E-AC3/DTS/TrueHD without a hardware decoder) -
     * the signal for the web engine to reopen the AAC-transcoded master. Reads
     * the player, so only call from the main thread (onPlayerError runs there). */
    private fun audioUnsupported(): Boolean {
        var sawAudio = false
        for (group in player.currentTracks.groups) {
            if (group.type != C.TRACK_TYPE_AUDIO) continue
            sawAudio = true
            for (i in 0 until group.length) if (group.isTrackSupported(i)) return false
        }
        return sawAudio
    }

    /** Publish the "continue watching" list into the launcher's system Watch Next
     * row (`[{id,title,subtitle?,imageUrl?,progressMs,durationMs,kind}]`). Runs
     * off the JS thread (provider I/O). Passing `[]` clears the row (sign-out). */
    @JavascriptInterface
    fun setContinueWatching(json: String) {
        val ctx = activity.applicationContext
        Log.i(TAG, "setContinueWatching: ${json.length} chars")
        Thread { WatchNext.sync(ctx, json) }.start()
    }

    /** Publish the recently-added + suggested titles to the KROMA preview channel
     * (a dedicated row on the launcher home). `[{id,title,subtitle?,imageUrl?,
     * kind}]`; `[]` clears it. Runs off the JS thread (provider I/O). */
    @JavascriptInterface
    fun setHomeChannel(json: String) {
        val ctx = activity.applicationContext
        Log.i(TAG, "setHomeChannel: ${json.length} chars")
        Thread { HomeChannel.sync(ctx, json) }.start()
    }

    /** Audio filter / volume normalizer (0 off, 1 standard, 2 night): a
     * single-band DynamicsProcessing compressor + safety limiter on the player's
     * audio session, tuned to MATCH the web client's Web Audio compressor so
     * every engine sounds the same (standard = 4:1 at -24 dB with make-up gain,
     * night = 8:1 at -28 dB with below-unity make-up so it is never louder than
     * off). Best effort: needs API 28+ and a decoded (non-passthrough) track;
     * anything else leaves the audio untouched, logs why and reports it back to
     * the page (audioFilterSupported). */
    private fun applyFilter(level: Int) {
        filterLevel = level
        dynamics?.release()
        dynamics = null
        filterChannels = 0
        if (level == 0 || Build.VERSION.SDK_INT < Build.VERSION_CODES.P) return
        val session = player.audioSessionId
        if (session == C.AUDIO_SESSION_ID_UNSET) {
            // Passthrough to an AVR: no decoded session to hook the effect onto.
            setFilterSupported(false) // reattached on the id event
            return
        }
        try {
            val night = level == 2
            // The REAL channel count, not a stereo guess: on 5.1/7.1 content a
            // 2-channel config leaves the surround channels (the loud ones night
            // mode exists for) untouched. Unknown format = the stereo default.
            val channels = player.audioFormat?.channelCount?.takeIf { it > 0 } ?: 2
            val config = DynamicsProcessing.Config.Builder(
                DynamicsProcessing.VARIANT_FAVOR_TIME_RESOLUTION,
                channels, // per-channel params are set all-channels below
                false, 0, // no pre-EQ
                true, 1,  // one full-range MBC band = the compressor
                false, 0, // no post-EQ
                true,     // limiter
            ).build()
            val dp = DynamicsProcessing(0, session, config)
            val band = DynamicsProcessing.MbcBand(
                true, 20000f,                  // enabled, cutoff (full range)
                if (night) 4f else 10f,        // attack ms
                250f,                          // release ms
                if (night) 8f else 4f,         // ratio
                if (night) -28f else -24f,     // threshold dB
                if (night) 5f else 6f,         // knee dB
                -90f, 1f,                      // noise gate + expander off
                0f,                            // pre gain dB
                if (night) -1f else 3f,        // post gain dB (0.9x / 1.4x)
            )
            dp.setMbcBandAllChannelsTo(0, band)
            // Backstop against make-up gain pushing a peak into clipping.
            dp.setLimiterAllChannelsTo(
                DynamicsProcessing.Limiter(true, true, 0, 1f, 60f, 10f, -2f, 0f),
            )
            dp.enabled = true
            dynamics = dp
            filterChannels = channels
            setFilterSupported(true)
        } catch (e: Exception) {
            // Passthrough audio, a device without the effect, or a ROM enforcing
            // MODIFY_AUDIO_SETTINGS: leave the audio clean, but never silently -
            // a swallowed failure here reads as a dead "Nuit" toggle.
            Log.w(TAG, "audio filter unavailable (level=$level, session=$session)", e)
            setFilterSupported(false)
        }
    }

    /** Latch the filter capability and push it to the page when it flips, so a UI
     * that asked before playback can drop the control once we know better. */
    private fun setFilterSupported(supported: Boolean) {
        if (filterSupported == supported) return
        filterSupported = supported
        emit(JSONObject().put("t", "filterSupported").put("supported", supported))
    }

    /** Shrink/restore the ACTIVE video plane: resize it to a fraction-rect of its
     * (full-screen FrameLayout) parent so the video lands in the settings card; a
     * `rect` with no bounds restores fullscreen (MATCH_PARENT). Targets the libVLC
     * SurfaceView when VLC is driving (resizing it fires surfaceChanged, which feeds
     * VLC the new render size) and the ExoPlayer PlayerView otherwise. */
    private fun setRect(cmd: JSONObject) {
        val target: View = if (vlcActive) vlcSurface else playerView
        val parent = target.parent as? android.view.View ?: return
        val pw = parent.width
        val ph = parent.height
        val lp = target.layoutParams as? android.widget.FrameLayout.LayoutParams ?: return
        if (cmd.has("w") && pw > 0 && ph > 0) {
            lp.width = (cmd.optDouble("w", 1.0) * pw).toInt()
            lp.height = (cmd.optDouble("h", 1.0) * ph).toInt()
            lp.leftMargin = (cmd.optDouble("x", 0.0) * pw).toInt()
            lp.topMargin = (cmd.optDouble("y", 0.0) * ph).toInt()
            lp.gravity = android.view.Gravity.TOP or android.view.Gravity.START
        } else {
            lp.width = android.widget.FrameLayout.LayoutParams.MATCH_PARENT
            lp.height = android.widget.FrameLayout.LayoutParams.MATCH_PARENT
            lp.leftMargin = 0
            lp.topMargin = 0
        }
        target.layoutParams = lp
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
        main.post {
            dynamics?.release()
            dynamics = null
            player.release()
            vlc?.release()
            vlc = null
        }
    }
}
