// Publishes KROMA "preview channels" - the named rows on the Android TV / Google
// TV launcher home (distinct from the system "Continue watching" Watch Next row;
// see WatchNext.kt). One channel per home section ("Recently added", "For you",
// ...), so the launcher shows several KROMA rows like the Tizen shortcuts.
//
// Channels are keyed by ROW INDEX (kroma:row:0..N), NOT by the server's section id
// (those aren't stable across launches - themed rows are regenerated - which would
// mint a brand-new channel every time and pile up dozens of duplicates). A fixed
// per-slot key lets each sync reuse the same channel: update its name + programs in
// place, and delete any slot no longer used. Enumeration is a direct provider query
// (PreviewChannelHelper.getAllChannels proved unreliable - it can miss our own rows,
// which is what let the duplicates accumulate).
//
// Each program deep-links back via `kroma://item/<id>`. Note: only the first channel
// an app publishes is shown automatically; the rest need the user to enable them via
// the launcher's "Customize channels". A big featured hero is NOT available to
// third-party apps on the Google TV home.
package tv.kroma.androidtv

import android.content.ContentValues
import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.net.Uri
import android.util.Log
import androidx.core.content.ContextCompat
import androidx.tvprovider.media.tv.PreviewChannel
import androidx.tvprovider.media.tv.PreviewChannelHelper
import androidx.tvprovider.media.tv.PreviewProgram
import androidx.tvprovider.media.tv.TvContractCompat
import org.json.JSONArray
import org.json.JSONObject

object HomeChannel {
    private const val TAG = "KromaHomeChannel"
    private const val KEY_PREFIX = "kroma:row:"

    /** Sync a list of launcher rows: `[{title, items:[{id,title,subtitle?,
     * imageUrl?,kind}]}]`, in display order. One preview channel per entry, keyed by
     * ROW INDEX so it reuses the same channel across syncs. `[]` clears every KROMA
     * channel. */
    @Synchronized
    fun sync(context: Context, json: String) {
        val specs = try {
            JSONArray(json)
        } catch (e: Exception) {
            Log.w(TAG, "bad home-channel payload", e)
            return
        }
        try {
            val helper = PreviewChannelHelper(context)
            val existing = ourChannels(context) // channelId -> key
            val byKey = HashMap<String, Long>()
            existing.forEach { (id, key) -> if (key.isNotEmpty()) byKey[key] = id }

            val wantedKeys = HashSet<String>()
            for (i in 0 until specs.length()) {
                val spec = specs.optJSONObject(i) ?: continue
                val key = KEY_PREFIX + i
                wantedKeys.add(key)
                val title = spec.optString("title", "KROMA")
                val items = spec.optJSONArray("items") ?: JSONArray()
                val channelId = byKey[key]
                    ?: publishChannel(context, helper, key, title, makeDefault = byKey.isEmpty() && i == 0)
                // Refresh the title (the section name can change) and mark the channel
                // browsable - so it shows on the home without the user toggling it in
                // "Customize channels" - in a single provider write. See applyChannel.
                applyChannel(context, channelId, title)
                reconcilePrograms(context, channelId, items)
            }

            // Delete stale rows (a slot we no longer publish) and the legacy pile-up
            // (the old single empty-key "KROMA" channel / retired row slots). Scope to
            // OUR keys (kroma:row:* or the legacy empty key) so a channel some other
            // code might publish for this package is never collateral damage.
            var removed = 0
            existing.forEach { (id, key) ->
                if (key !in wantedKeys && (key.startsWith(KEY_PREFIX) || key.isEmpty())) {
                    deleteChannel(context, id)
                    removed++
                }
            }
            Log.i(TAG, "home-channel synced ${wantedKeys.size} row(s), removed $removed stale")
        } catch (e: Exception) {
            Log.w(TAG, "home-channel sync failed", e)
        }
    }

    /** Remove every KROMA preview channel (called on sign-out). */
    @Synchronized
    fun clear(context: Context) {
        runCatching { ourChannels(context).keys.forEach { deleteChannel(context, it) } }
    }

    /** Our published channels (channelId -> internalProviderId key), read straight
     * from the provider (package-scoped, so every row is ours). */
    private fun ourChannels(context: Context): Map<Long, String> {
        val out = HashMap<Long, String>()
        val projection = arrayOf(
            TvContractCompat.Channels._ID,
            TvContractCompat.Channels.COLUMN_INTERNAL_PROVIDER_ID,
        )
        context.contentResolver.query(
            TvContractCompat.Channels.CONTENT_URI,
            projection,
            null,
            null,
            null,
        )?.use { c ->
            while (c.moveToNext()) out[c.getLong(0)] = c.getString(1) ?: ""
        }
        return out
    }

    /** Publish a new named channel keyed by `key`; make the very first one the
     * default (auto-shown) and request the rest browsable so the user can add them. */
    private fun publishChannel(
        context: Context,
        helper: PreviewChannelHelper,
        key: String,
        title: String,
        makeDefault: Boolean,
    ): Long {
        val channel = PreviewChannel.Builder()
            .setDisplayName(title)
            .setInternalProviderId(key)
            .setAppLinkIntentUri(Uri.parse("kroma://home"))
            .apply { bannerBitmap(context)?.let { setLogo(it) } }
            .build()
        val id = if (makeDefault) helper.publishDefaultChannel(channel) else helper.publishChannel(channel)
        runCatching { TvContractCompat.requestChannelBrowsable(context, id) }
        return id
    }

    /** Set the display name (the section title can change) and mark the channel
     * browsable (best effort: some launchers honor an app marking its OWN channel
     * browsable and show it without the user opting in via "Customize channels";
     * strict ones ignore it and still require the manual toggle) - both in a single
     * provider write. */
    private fun applyChannel(context: Context, channelId: Long, title: String) {
        runCatching {
            val values = ContentValues().apply {
                put(TvContractCompat.Channels.COLUMN_DISPLAY_NAME, title)
                put(TvContractCompat.Channels.COLUMN_BROWSABLE, 1)
            }
            context.contentResolver.update(TvContractCompat.buildChannelUri(channelId), values, null, null)
        }
    }

    private fun deleteChannel(context: Context, channelId: Long) {
        // Deleting a channel cascades to its programs.
        runCatching { context.contentResolver.delete(TvContractCompat.buildChannelUri(channelId), null, null) }
    }

    /** Insert/refresh this channel's programs and drop the ones no longer listed. */
    private fun reconcilePrograms(context: Context, channelId: Long, items: JSONArray) {
        val wanted = LinkedHashMap<String, JSONObject>()
        for (i in 0 until items.length()) {
            val o = items.optJSONObject(i) ?: continue
            val id = o.optString("id")
            if (id.isNotEmpty()) wanted[id] = o
        }
        val existing = existingPrograms(context, channelId) // itemId -> [programId]
        for ((itemId, o) in wanted) {
            for (rowId in existing[itemId].orEmpty()) removeRow(context, rowId)
            insertRow(context, channelId, itemId, o)
        }
        for ((itemId, rows) in existing) {
            if (!wanted.containsKey(itemId)) for (rowId in rows) removeRow(context, rowId)
        }
    }

    private fun insertRow(context: Context, channelId: Long, itemId: String, o: JSONObject) {
        val type =
            if (o.optString("kind") == "episode") TvContractCompat.PreviewPrograms.TYPE_TV_EPISODE
            else TvContractCompat.PreviewPrograms.TYPE_MOVIE
        val builder = PreviewProgram.Builder()
            .setChannelId(channelId)
            .setType(type)
            .setTitle(o.optString("title"))
            .setInternalProviderId(itemId)
            .setIntentUri(Uri.parse("kroma://item/$itemId"))
        o.optString("subtitle").takeIf { it.isNotEmpty() }?.let { builder.setDescription(it) }
        o.optString("imageUrl").takeIf { it.isNotEmpty() }?.let {
            builder.setPosterArtUri(Uri.parse(it))
            builder.setPosterArtAspectRatio(TvContractCompat.PreviewPrograms.ASPECT_RATIO_16_9)
        }
        context.contentResolver.insert(
            TvContractCompat.PreviewPrograms.CONTENT_URI,
            builder.build().toContentValues(),
        )
    }

    private fun removeRow(context: Context, rowId: Long) {
        context.contentResolver.delete(TvContractCompat.buildPreviewProgramUri(rowId), null, null)
    }

    /** One channel's published programs, grouped by item id (internalProviderId). */
    private fun existingPrograms(context: Context, channelId: Long): Map<String, List<Long>> {
        val out = HashMap<String, MutableList<Long>>()
        val projection = arrayOf(
            TvContractCompat.PreviewPrograms._ID,
            TvContractCompat.PreviewPrograms.COLUMN_INTERNAL_PROVIDER_ID,
        )
        context.contentResolver.query(
            TvContractCompat.buildPreviewProgramsUriForChannel(channelId),
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

    /** The app banner drawable as a bitmap, for the channel logo. */
    private fun bannerBitmap(context: Context): Bitmap? {
        val d = ContextCompat.getDrawable(context, R.drawable.tv_banner) ?: return null
        val w = d.intrinsicWidth.coerceAtLeast(1)
        val h = d.intrinsicHeight.coerceAtLeast(1)
        val bmp = Bitmap.createBitmap(w, h, Bitmap.Config.ARGB_8888)
        d.setBounds(0, 0, w, h)
        d.draw(Canvas(bmp))
        return bmp
    }
}
