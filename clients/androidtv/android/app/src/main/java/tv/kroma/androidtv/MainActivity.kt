// KROMA Android TV shell: a transparent WebView (the shared @kroma/tv UI, bundled
// as assets) floating over a native media3/ExoPlayer plane - the same "video
// plane behind the page" model as Tizen AVPlay and the Steam Deck's mpv.
//
//  - The web app detects the injected `__KROMA_ANDROID__` bridge (ExoBridge) and
//    routes playback through its ExoEngine; everything else (navigation, auth,
//    discovery, subtitles) is the plain web app.
//  - D-pad and OK arrive as normal DOM key events through the WebView. BACK and
//    the media-transport keys are consumed by Android before the WebView sees
//    them, so they are re-injected as synthetic `keydown`s with the key names
//    @kroma/core's remote mapping already understands.
package tv.kroma.androidtv

import android.annotation.SuppressLint
import android.app.Activity
import android.graphics.Color
import android.os.Bundle
import android.view.KeyEvent
import android.view.ViewGroup.LayoutParams.MATCH_PARENT
import android.webkit.WebView
import android.widget.FrameLayout
import androidx.media3.ui.PlayerView

class MainActivity : Activity() {
    private lateinit var webView: WebView
    private lateinit var bridge: ExoBridge

    @SuppressLint("SetJavaScriptEnabled")
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        val root = FrameLayout(this)
        val playerView = PlayerView(this).apply {
            useController = false // KROMA draws its own chrome in the WebView
            layoutParams = FrameLayout.LayoutParams(MATCH_PARENT, MATCH_PARENT)
        }
        webView = WebView(this).apply {
            setBackgroundColor(Color.TRANSPARENT) // video plane shows through
            layoutParams = FrameLayout.LayoutParams(MATCH_PARENT, MATCH_PARENT)
            settings.javaScriptEnabled = true
            settings.domStorageEnabled = true // session/servers persist in localStorage
            settings.mediaPlaybackRequiresUserGesture = false
            settings.allowFileAccess = true // the app is packaged as file:// assets
        }
        bridge = ExoBridge(this, webView, playerView)
        webView.addJavascriptInterface(bridge, "__KROMA_ANDROID__")

        root.addView(playerView)
        root.addView(webView)
        setContentView(root)

        webView.loadUrl("file:///android_asset/kroma/index.html")
    }

    // Keys Android consumes before the WebView: re-injected as the DOM key names
    // @kroma/core's KEY_NAMES map resolves (Escape -> Back, Media* -> transport).
    private val domKeys = mapOf(
        KeyEvent.KEYCODE_BACK to "Escape",
        KeyEvent.KEYCODE_MEDIA_PLAY to "MediaPlay",
        KeyEvent.KEYCODE_MEDIA_PAUSE to "MediaPause",
        KeyEvent.KEYCODE_MEDIA_PLAY_PAUSE to "MediaPlayPause",
        KeyEvent.KEYCODE_MEDIA_STOP to "MediaStop",
        KeyEvent.KEYCODE_MEDIA_REWIND to "MediaRewind",
        KeyEvent.KEYCODE_MEDIA_FAST_FORWARD to "MediaFastForward",
        KeyEvent.KEYCODE_MEDIA_NEXT to "MediaTrackNext",
        KeyEvent.KEYCODE_MEDIA_PREVIOUS to "MediaTrackPrevious",
    )

    override fun dispatchKeyEvent(event: KeyEvent): Boolean {
        val key = domKeys[event.keyCode] ?: return super.dispatchKeyEvent(event)
        if (event.action == KeyEvent.ACTION_DOWN) {
            webView.evaluateJavascript(
                "window.dispatchEvent(new KeyboardEvent('keydown',{key:'$key',bubbles:true}))",
                null,
            )
        }
        return true
    }

    override fun onStop() {
        // Good TV citizenship: never keep playing audio behind the launcher.
        bridge.pauseForBackground()
        super.onStop()
    }

    override fun onDestroy() {
        bridge.release()
        super.onDestroy()
    }
}
