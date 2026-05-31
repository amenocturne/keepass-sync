package space.amenocturne.keepasssync

import android.content.Context
import android.net.Uri

class SyncPreferences(context: Context) {
    private val prefs = context.getSharedPreferences("keepass-sync", Context.MODE_PRIVATE)

    var localDbUri: Uri?
        get() = prefs.getString("local_db_uri", null)?.let(Uri::parse)
        set(value) = prefs.edit().putString("local_db_uri", value?.toString()).apply()

    var remoteRootUri: Uri?
        get() = prefs.getString("remote_root_uri", null)?.let(Uri::parse)
        set(value) = prefs.edit().putString("remote_root_uri", value?.toString()).apply()

    var deviceId: String
        get() = prefs.getString("device_id", null) ?: defaultDeviceId()
        set(value) = prefs.edit().putString("device_id", value).apply()

    var baseRevision: Revision?
        get() = prefs.getString("base_revision", null)?.let(::Revision)
        set(value) = prefs.edit().putString("base_revision", value?.value).apply()

    private fun defaultDeviceId(): String =
        android.os.Build.MODEL
            .lowercase()
            .replace(Regex("[^a-z0-9-]+"), "-")
            .trim('-')
            .ifBlank { "android" }
}
