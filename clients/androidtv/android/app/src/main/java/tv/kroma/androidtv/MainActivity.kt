// KROMA Android TV shell: a transparent WebView (the shared @kroma/tv UI, bundled
// as assets) floating over a native media3/ExoPlayer plane - the same "video
// plane behind the page" model as Tizen AVPlay and the Steam Deck's mpv.
//
//  - The web app detects the injected `__KROMA_ANDROID__` bridge (ExoBridge) and
//    routes playback through its ExoEngine; everything else (navigation, auth,
//    discovery, subtitles) is the plain web app.
//  - The bundle is served over the https://appassets.androidplatform.net virtual
//    origin (WebViewAssetLoader), NOT file://: the Vite build emits ES-module
//    scripts (`<script type="module">` + dynamic import()), which a WebView
//    refuses to load from file:// (module scripts require CORS, and file:// is a
//    null origin) - the app then never renders and the black ExoPlayer plane
//    shows through (a full black screen). A real https origin fixes that, and as
//    a bonus makes it a secure context (Web Crypto / passkeys work).
//  - D-pad and OK arrive as normal DOM key events through the WebView. BACK and
//    the media-transport keys are consumed by Android before the WebView sees
//    them, so they are re-injected as synthetic `keydown`s with the key names
//    @kroma/core's remote mapping already understands.
package tv.kroma.androidtv

import android.annotation.SuppressLint
import android.app.Activity
import android.content.Intent
import android.graphics.Color
import android.net.Uri
import android.os.Bundle
import android.view.KeyEvent
import android.view.SurfaceView
import android.view.View
import android.view.ViewGroup.LayoutParams.MATCH_PARENT
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebSettings
import android.webkit.WebView
import android.widget.FrameLayout
import androidx.media3.ui.PlayerView
import androidx.webkit.WebViewAssetLoader
import androidx.webkit.WebViewClientCompat
import org.json.JSONObject

/** The bundled web app, served over the WebViewAssetLoader virtual https origin. */
private const val APP_URL = "https://appassets.androidplatform.net/assets/kroma/index.html"

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
        // The libVLC software-decode plane, shown only when ExoPlayer can't decode
        // the video and we hand off to VLC (see ExoBridge). Hidden otherwise so it
        // doesn't cover the ExoPlayer surface.
        val vlcSurface = SurfaceView(this).apply {
            layoutParams = FrameLayout.LayoutParams(MATCH_PARENT, MATCH_PARENT)
            visibility = View.GONE
        }
        // Serve the bundled assets over the virtual https origin so ES-module
        // scripts load (see the file header). `/assets/` maps to android_asset/.
        val assetLoader = WebViewAssetLoader.Builder()
            .addPathHandler("/assets/", WebViewAssetLoader.AssetsPathHandler(this))
            .build()

        webView = WebView(this).apply {
            setBackgroundColor(Color.TRANSPARENT) // video plane shows through
            layoutParams = FrameLayout.LayoutParams(MATCH_PARENT, MATCH_PARENT)
            settings.javaScriptEnabled = true
            settings.domStorageEnabled = true // session/servers persist in localStorage
            settings.mediaPlaybackRequiresUserGesture = false
            // Honor the app's `<meta viewport width=1920>` and scale that fixed
            // 1080p layout to fill the panel. Without this the WebView lays out at
            // the display's density width (a 4K TV at density 640 = only 960dp),
            // so a 1920-designed UI renders ~2x too big ("everything feels huge").
            settings.useWideViewPort = true
            settings.loadWithOverviewMode = true
            // The app runs on the https appassets origin but a LAN KROMA server is
            // usually plain http; allow that cross-scheme fetch (mixed content).
            settings.mixedContentMode = WebSettings.MIXED_CONTENT_ALWAYS_ALLOW
            webViewClient = object : WebViewClientCompat() {
                override fun shouldInterceptRequest(
                    view: WebView,
                    request: WebResourceRequest,
                ): WebResourceResponse? = assetLoader.shouldInterceptRequest(request.url)
            }
        }
        bridge = ExoBridge(this, webView, playerView, vlcSurface)
        webView.addJavascriptInterface(bridge, "__KROMA_ANDROID__")

        root.addView(playerView)
        root.addView(vlcSurface) // above the ExoPlayer plane, below the WebView chrome
        root.addView(webView)
        setContentView(root)

        // A cold launch from a Watch Next `kroma://item/<id>` deep link carries the
        // id as a query param the web app reads on boot; a normal launch omits it.
        val deepLink = itemIdFromDeepLink(intent?.data)
        val url = StringBuilder(APP_URL)
        if (deepLink != null) url.append("?deeplink=").append(Uri.encode(deepLink))
        webView.loadUrl(url.toString())
    }

    // A deep link arriving while the app is already running (warm start): hand the
    // id to the web app via a DOM event instead of reloading the whole page.
    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        val id = itemIdFromDeepLink(intent.data) ?: return
        if (!::webView.isInitialized) return
        webView.evaluateJavascript(
            "window.dispatchEvent(new CustomEvent('kroma-deeplink',{detail:${JSONObject.quote(id)}}))",
            null,
        )
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
