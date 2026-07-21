// Publishes KROMA's "continue watching" list into the Android TV / Google TV
// system "Continue watching" (Watch Next) row on the launcher home screen - the
// platform equivalent of the Tizen Smart Hub carousel.
//
// The open app pushes its list here (ExoBridge.setContinueWatching); the entries
// persist on the home screen after the app closes. Each program deep-links back
// via `kroma://item/<id>`, which MainActivity routes into the web app. We track
// the program ids we published in SharedPreferences so a later sync can remove
// stale rows without querying the provider.
package tv.kroma.androidtv

import android.content.ContentUris
import android.content.Context
import android.net.Uri
import android.util.Log
import androidx.tvprovider.media.tv.TvContractCompat
import androidx.tvprovider.media.tv.WatchNextProgram
import org.json.JSONArray
import org.json.JSONObject

object WatchNext {
    private const val TAG = "KromaWatchNext"
    private const val PREFS = "kroma_watch_next"
    // itemId -> Watch Next program row id we inserted (so we can update/remove it).
    private const val KEY_IDS = "program_ids"

    /**
     * Sync the given `continue watching` list (a JSON array of
     * `{id,title,subtitle?,imageUrl?,progressMs,durationMs,kind}`) into the Watch
     * Next row: insert new items, refresh existing ones, and drop rows that are
     * no longer in the list. Best effort - a provider hiccup is logged, never fatal.
     */
    fun sync(context: Context, json: String) {
        val arr = try {
            JSONArray(json)
        } catch (e: Exception) {
            Log.w(TAG, "bad continue-watching payload", e)
            return
        }
        val prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        val known = readMap(prefs.getString(KEY_IDS, null))
        val wanted = LinkedHashMap<String, JSONObject>()
        for (i in 0 until arr.length()) {
            val o = arr.optJSONObject(i) ?: continue
            val id = o.optString("id")
            if (id.isNotEmpty()) wanted[id] = o
        }

        val next = HashMap<String, Long>()
        try {
            // Drop rows no longer wanted.
            for ((itemId, rowId) in known) {
                if (!wanted.containsKey(itemId)) removeRow(context, rowId)
            }
            // Insert / refresh the wanted ones (delete-then-insert keeps it simple:
            // a Watch Next program has no stable natural key we can upsert on).
            for ((itemId, o) in wanted) {
                known[itemId]?.let { removeRow(context, it) }
                val rowId = insertRow(context, itemId, o)
                if (rowId > 0) next[itemId] = rowId
            }
        } catch (e: Exception) {
            Log.w(TAG, "watch-next sync failed", e)
        }
        prefs.edit().putString(KEY_IDS, writeMap(next)).apply()
    }

    /** Remove every KROMA Watch Next row (called on sign-out). */
    fun clear(context: Context) {
        val prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        for ((_, rowId) in readMap(prefs.getString(KEY_IDS, null))) {
            runCatching { removeRow(context, rowId) }
        }
        prefs.edit().remove(KEY_IDS).apply()
    }

    private fun insertRow(context: Context, itemId: String, o: JSONObject): Long {
        val type =
            if (o.optString("kind") == "episode") TvContractCompat.WatchNextPrograms.TYPE_TV_EPISODE
            else TvContractCompat.WatchNextPrograms.TYPE_MOVIE
        val builder = WatchNextProgram.Builder()
            .setType(type)
            .setWatchNextType(TvContractCompat.WatchNextPrograms.WATCH_NEXT_TYPE_CONTINUE)
            .setTitle(o.optString("title"))
            .setInternalProviderId(itemId)
            .setLastEngagementTimeUtcMillis(
                o.optLong("updatedAtMs", System.currentTimeMillis()),
            )
            .setIntentUri(Uri.parse("kroma://item/$itemId"))
        o.optString("subtitle").takeIf { it.isNotEmpty() }?.let { builder.setDescription(it) }
        o.optString("imageUrl").takeIf { it.isNotEmpty() }?.let {
            builder.setPosterArtUri(Uri.parse(it))
            builder.setPosterArtAspectRatio(TvContractCompat.PreviewPrograms.ASPECT_RATIO_16_9)
        }
        val dur = o.optLong("durationMs", 0)
        val pos = o.optLong("progressMs", 0)
        if (dur > 0) builder.setDurationMillis(dur.toInt())
        if (pos > 0) builder.setLastPlaybackPositionMillis(pos.toInt())

        val uri = context.contentResolver.insert(
            TvContractCompat.WatchNextPrograms.CONTENT_URI,
            builder.build().toContentValues(),
        ) ?: return -1
        return ContentUris.parseId(uri)
    }

    private fun removeRow(context: Context, rowId: Long) {
        val uri = TvContractCompat.buildWatchNextProgramUri(rowId)
        context.contentResolver.delete(uri, null, null)
    }

    // itemId=rowId pairs, one per line (SharedPreferences string). Tiny + robust.
    private fun readMap(s: String?): LinkedHashMap<String, Long> {
        val m = LinkedHashMap<String, Long>()
        if (s.isNullOrEmpty()) return m
        for (line in s.split('\n')) {
            val eq = line.lastIndexOf('=')
            if (eq <= 0) continue
            line.substring(eq + 1).toLongOrNull()?.let { m[line.substring(0, eq)] = it }
        }
        return m
    }

    private fun writeMap(m: Map<String, Long>): String =
        m.entries.joinToString("\n") { "${it.key}=${it.value}" }
}

/** The item id carried by a `kroma://item/<id>` deep link, or null. */
fun itemIdFromDeepLink(uri: Uri?): String? {
    if (uri == null || uri.scheme != "kroma") return null
    // kroma://item/<id>  ->  host = "item", path = "/<id>"
    if (uri.host != "item") return null
    return uri.lastPathSegment?.takeIf { it.isNotEmpty() }
}
