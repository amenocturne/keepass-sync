package space.amenocturne.keepasssync

import android.content.ContentResolver
import android.net.Uri
import java.time.Instant

class AndroidSyncClient(
    private val resolver: ContentResolver,
    private val prefs: SyncPreferences,
) {
    fun sync(): SyncResult {
        val localUri = prefs.localDbUri ?: error("choose a local KDBX file first")
        val remoteUri = prefs.remoteRootUri ?: error("choose a remote root folder first")
        val remote = DocumentTreeRemote(resolver, remoteUri)
        val localBytes = readLocal(localUri)
        val localRevision = Revision.fromBytes(localBytes)
        val remoteRevision = remoteRevision(remote)

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
                publish(remote, localBytes, localRevision)
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
                val canonical = remote.read(CANONICAL_DB) ?: error("remote canonical database is missing")
                writeLocal(localUri, canonical)
                prefs.baseRevision = Revision.fromBytes(canonical)
                SyncResult(action, "Pulled remote canonical database.")
            }

            SyncAction.PreserveIncoming -> {
                val incomingPath = listOf("incoming", prefs.deviceId, "${localRevision.safeName()}.kdbx")
                remote.write(incomingPath, localBytes)
                SyncResult(action, "Saved divergent local copy for desktop merge.")
            }
        }
    }

    private fun publish(remote: DocumentTreeRemote, bytes: ByteArray, revision: Revision) {
        remote.write(CANONICAL_DB, bytes)
        remote.write(
            MANIFEST,
            Manifest(
                revision = revision,
                updatedAt = Instant.now().toString(),
                updatedBy = prefs.deviceId,
            ).toJson().encodeToByteArray(),
        )
    }

    private fun remoteRevision(remote: DocumentTreeRemote): Revision? {
        val manifestBytes = remote.read(MANIFEST) ?: return null
        val canonicalBytes = remote.read(CANONICAL_DB) ?: error("manifest exists but canonical database is missing")
        val manifest = Manifest.parse(manifestBytes.decodeToString())
        val actual = Revision.fromBytes(canonicalBytes)
        require(manifest.revision == actual) {
            "manifest revision ${manifest.revision} does not match canonical hash $actual"
        }
        return manifest.revision
    }

    private fun readLocal(uri: Uri): ByteArray =
        resolver.openInputStream(uri)?.use { it.readBytes() }
            ?: error("failed to read local database")

    private fun writeLocal(uri: Uri, bytes: ByteArray) {
        resolver.openOutputStream(uri, "rwt")?.use { it.write(bytes) }
            ?: error("failed to write local database")
    }

    companion object {
        private val CANONICAL_DB = listOf("canonical", "passwords.kdbx")
        private val MANIFEST = listOf("canonical", "manifest.json")
    }
}

data class SyncResult(
    val action: SyncAction,
    val message: String,
)
