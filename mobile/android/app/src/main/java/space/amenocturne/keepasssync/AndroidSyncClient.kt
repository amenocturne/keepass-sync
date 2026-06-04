package space.amenocturne.keepasssync

import android.content.ContentResolver
import android.net.Uri

class AndroidSyncClient(
    private val resolver: ContentResolver,
    private val prefs: SyncPreferences,
) {
    fun sync(): SyncResult {
        val localUri = prefs.localDbUri ?: error("choose a local KDBX file first")
        val remote = HttpRemote(prefs.endpointUrl, prefs.authToken)
        val localBytes = readLocal(localUri)
        val localRevision = Revision.fromBytes(localBytes)
        val remoteRevision = remote.manifest()?.revision

        val action = decideSync(
            SyncInputs(
                localRevision = localRevision,
                baseRevision = prefs.baseRevision,
                remoteRevision = remoteRevision,
            ),
        )

        return when (action) {
            SyncAction.InitializeRemote,
            SyncAction.PublishLocal -> {
                val manifest = remote.publish(
                    bytes = localBytes,
                    revision = localRevision,
                    baseRevision = prefs.baseRevision,
                    deviceId = prefs.deviceId,
                )
                require(manifest.revision == localRevision) {
                    "server manifest revision ${manifest.revision} did not match uploaded revision $localRevision"
                }
                prefs.baseRevision = localRevision
                SyncResult(action, "Published local database as canonical.")
            }

            SyncAction.AdoptRemote -> {
                prefs.baseRevision = remoteRevision
                SyncResult(action, "Adopted existing remote canonical revision.")
            }

            SyncAction.Noop ->
                SyncResult(action, "Already synced.")

            SyncAction.PullRemote -> {
                val canonical = remote.canonical()
                writeLocal(localUri, canonical)
                prefs.baseRevision = Revision.fromBytes(canonical)
                SyncResult(action, "Pulled remote canonical database.")
            }

            SyncAction.PreserveIncoming -> {
                remote.preserveIncoming(localBytes, localRevision, prefs.deviceId)
                SyncResult(action, "Saved divergent local copy for desktop merge.")
            }
        }
    }

    private fun readLocal(uri: Uri): ByteArray =
        resolver.openInputStream(uri)?.use { it.readBytes() }
            ?: error("failed to read local database")

    private fun writeLocal(uri: Uri, bytes: ByteArray) {
        resolver.openOutputStream(uri, "rwt")?.use { it.write(bytes) }
            ?: error("failed to write local database")
    }

}

data class SyncResult(
    val action: SyncAction,
    val message: String,
)
