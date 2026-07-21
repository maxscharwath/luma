// Publishes KROMA's "continue watching" list into the Android TV / Google TV
// system "Continue watching" (Watch Next) row on the launcher home screen - the
// platform equivalent of the Tizen Smart Hub carousel.
//
// The open app pushes its list here (ExoBridge.setContinueWatching); the entries
// persist on the home screen after the app closes. Each program deep-links back
// via `kroma://item/<id>`, which MainActivity routes into the web app. Each sync
// reconciles against the rows actually published (queried from the provider), so
// it is idempotent and never leaves duplicates behind.
package tv.kroma.androidtv

import android.content.Context
import android.net.Uri
import android.util.Log
import androidx.tvprovider.media.tv.TvContractCompat
import androidx.tvprovider.media.tv.WatchNextProgram
import org.json.JSONArray
import org.json.JSONObject

object WatchNext {
    private const val TAG = "KromaWatchNext"

    /**
     * Sync the given `continue watching` list (a JSON array of
     * `{id,title,subtitle?,imageUrl?,progressMs,durationMs,kind}`) into the Watch
     * Next row: insert new items, refresh existing ones, and drop rows that are
     * no longer in the list. Best effort - a provider hiccup is logged, never fatal.
     */
    @Synchronized
    fun sync(context: Context, json: String) {
        val arr = try {
            JSONArray(json)
        } catch (e: Exception) {
            Log.w(TAG, "bad continue-watching payload", e)
            return
        }
        val wanted = LinkedHashMap<String, JSONObject>()
        for (i in 0 until arr.length()) {
            val o = arr.optJSONObject(i) ?: continue
            val id = o.optString("id")
            if (id.isNotEmpty()) wanted[id] = o
        }

        try {
            // Reconcile against what is ACTUALLY published, not a local record that
            // can go stale on reinstall or race with a concurrent sync (which was
            // duplicating rows). Query our existing rows grouped by item id, then:
            // delete every row for an unwanted id, and every DUPLICATE row for a
            // wanted id (keeping one to refresh).
            val existing = existingRows(context) // itemId -> [rowId, ...]
            for ((itemId, o) in wanted) {
                val rows = existing[itemId].orEmpty()
                // Drop all existing rows for this id, then insert one fresh (a Watch
                // Next program has no stable natural key to upsert on).
                for (rowId in rows) removeRow(context, rowId)
                insertRow(context, itemId, o)
            }
            for ((itemId, rows) in existing) {
                if (!wanted.containsKey(itemId)) for (rowId in rows) removeRow(context, rowId)
            }
            Log.i(TAG, "watch-next synced ${wanted.size} item(s)")
        } catch (e: Exception) {
            Log.w(TAG, "watch-next sync failed", e)
        }
    }

    /** Remove every KROMA Watch Next row (called on sign-out). */
    @Synchronized
    fun clear(context: Context) {
        runCatching {
            for ((_, rows) in existingRows(context)) for (rowId in rows) removeRow(context, rowId)
        }
    }

    /** Our currently-published Watch Next rows, grouped by item id (the
     * internalProviderId). A query returns only this app's own programs, so any
     * row we see is ours to reconcile. Duplicates for one id land in the list. */
    private fun existingRows(context: Context): Map<String, List<Long>> {
        val out = HashMap<String, MutableList<Long>>()
        val projection = arrayOf(
            TvContractCompat.WatchNextPrograms._ID,
            TvContractCompat.WatchNextPrograms.COLUMN_INTERNAL_PROVIDER_ID,
        )
        context.contentResolver.query(
            TvContractCompat.WatchNextPrograms.CONTENT_URI,
            projection,
            null,
            null,
            null,
        )?.use { c ->
            while (c.moveToNext()) {
                val rowId = c.getLong(0)
                val itemId = c.getString(1) ?: continue
                out.getOrPut(itemId) { mutableListOf() }.add(rowId)
            }
        }
        return out
    }

    private fun insertRow(context: Context, itemId: String, o: JSONObject) {
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

        context.contentResolver.insert(
            TvContractCompat.WatchNextPrograms.CONTENT_URI,
            builder.build().toContentValues(),
        )
    }

    private fun removeRow(context: Context, rowId: Long) {
        val uri = TvContractCompat.buildWatchNextProgramUri(rowId)
        context.contentResolver.delete(uri, null, null)
    }
}

/** The item id carried by a `kroma://item/<id>` deep link, or null. */
fun itemIdFromDeepLink(uri: Uri?): String? {
    if (uri == null || uri.scheme != "kroma") return null
    // kroma://item/<id>  ->  host = "item", path = "/<id>"
    if (uri.host != "item") return null
    return uri.lastPathSegment?.takeIf { it.isNotEmpty() }
}
