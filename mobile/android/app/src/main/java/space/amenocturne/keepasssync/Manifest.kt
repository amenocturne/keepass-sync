package space.amenocturne.keepasssync

import org.json.JSONObject

data class Manifest(
    val revision: Revision,
    val updatedAt: String,
    val updatedBy: String,
) {
    fun toJson(): String =
        JSONObject()
            .put("schema_version", SCHEMA_VERSION)
            .put("revision", revision.value)
            .put("updated_at", updatedAt)
            .put("updated_by", updatedBy)
            .toString(2)

    companion object {
        const val SCHEMA_VERSION = 1

        fun parse(json: String): Manifest {
            val obj = JSONObject(json)
            val schema = obj.getInt("schema_version")
            require(schema == SCHEMA_VERSION) { "unsupported manifest schema version: $schema" }
            return Manifest(
                revision = Revision(obj.getString("revision")),
                updatedAt = obj.getString("updated_at"),
                updatedBy = obj.getString("updated_by"),
            )
        }
    }
}
