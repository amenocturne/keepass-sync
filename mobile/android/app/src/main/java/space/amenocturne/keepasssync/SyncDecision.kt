package space.amenocturne.keepasssync

enum class SyncAction {
    InitializeRemote,
    AdoptRemote,
    Noop,
    PullRemote,
    PublishLocal,
    PreserveIncoming,
}

data class SyncInputs(
    val localRevision: Revision,
    val baseRevision: Revision?,
    val remoteRevision: Revision?,
)

fun decideSync(inputs: SyncInputs): SyncAction =
    when {
        inputs.baseRevision == null && inputs.remoteRevision == null ->
            SyncAction.InitializeRemote

        inputs.baseRevision == null && inputs.remoteRevision == inputs.localRevision ->
            SyncAction.AdoptRemote

        inputs.baseRevision == null ->
            SyncAction.PreserveIncoming

        inputs.remoteRevision == null ->
            error("remote canonical database is missing after initialization")

        else -> {
            val localChanged = inputs.localRevision != inputs.baseRevision
            val remoteChanged = inputs.remoteRevision != inputs.baseRevision
            when {
                !localChanged && !remoteChanged -> SyncAction.Noop
                !localChanged && remoteChanged -> SyncAction.PullRemote
                localChanged && !remoteChanged -> SyncAction.PublishLocal
                else -> SyncAction.PreserveIncoming
            }
        }
    }
