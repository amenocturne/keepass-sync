package space.amenocturne.keepasssync

import android.content.Context
import android.net.Uri

class SyncPreferences(context: Context) {
    private val prefs = context.getSharedPreferences("keepass-sync", Context.MODE_PRIVATE)

    var localDbUri: Uri?
        get() = prefs.getString("local_db_uri", null)?.let(Uri::parse)
        set(value) = prefs.edit().putString("local_db_uri", value?.toString()).apply()

    var deviceId: String
        get() = prefs.getString("device_id", null) ?: defaultDeviceId()
        set(value) = prefs.edit().putString("device_id", value).apply()

    var endpointUrl: String
        get() = prefs.getString("endpoint_url", null) ?: ""
        set(value) = prefs.edit().putString("endpoint_url", value).apply()

    var authToken: String
        get() = prefs.getString("auth_token", null) ?: ""
        set(value) = prefs.edit().putString("auth_token", value).apply()

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
