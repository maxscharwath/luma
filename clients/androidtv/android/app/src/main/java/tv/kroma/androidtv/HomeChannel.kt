// Publishes a KROMA "preview channel" - a dedicated row on the Android TV /
// Google TV launcher home for recently-added + suggested titles (distinct from
// the system "Continue watching" Watch Next row; see WatchNext.kt). The first
// channel an app publishes is its default channel and is shown automatically.
//
// The open app pushes the list here (ExoBridge.setHomeChannel); each program
// deep-links back via `kroma://item/<id>`. Like WatchNext, each sync reconciles
// against the programs actually published so it never leaves duplicates.
package tv.kroma.androidtv

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
    private const val PREFS = "kroma_home_channel"
    private const val KEY_CHANNEL = "channel_id"

    @Synchronized
    fun sync(context: Context, json: String) {
        val arr = try {
            JSONArray(json)
        } catch (e: Exception) {
            Log.w(TAG, "bad home-channel payload", e)
            return
        }
        try {
            val helper = PreviewChannelHelper(context)
            val channelId = ensureChannel(context, helper)
            if (channelId < 0L) return

            val wanted = LinkedHashMap<String, JSONObject>()
            for (i in 0 until arr.length()) {
                val o = arr.optJSONObject(i) ?: continue
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
        } catch (e: Exception) {
            Log.w(TAG, "home-channel sync failed", e)
        }
    }

    /** Get (or publish) the default KROMA channel, remembering its id. */
    private fun ensureChannel(context: Context, helper: PreviewChannelHelper): Long {
        val prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        val saved = prefs.getLong(KEY_CHANNEL, -1L)
        if (saved >= 0L && runCatching { helper.getPreviewChannel(saved) }.getOrNull() != null) {
            return saved
        }
        val channel = PreviewChannel.Builder()
            .setDisplayName("KROMA")
            .setAppLinkIntentUri(Uri.parse("kroma://home"))
            .apply { bannerBitmap(context)?.let { setLogo(it) } }
            .build()
        val id = helper.publishDefaultChannel(channel)
        // The default channel shows without a user prompt; make sure it is browsable.
        runCatching { TvContractCompat.requestChannelBrowsable(context, id) }
        prefs.edit().putLong(KEY_CHANNEL, id).apply()
        return id
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

    /** Our channel's published programs, grouped by item id (internalProviderId). */
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
