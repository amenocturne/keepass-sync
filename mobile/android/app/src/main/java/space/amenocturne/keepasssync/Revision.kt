package space.amenocturne.keepasssync

import java.security.MessageDigest

@JvmInline
value class Revision(val value: String) {
    init {
        require(value.startsWith("sha256:")) { "revision must start with sha256:" }
        require(value.removePrefix("sha256:").length == 64) { "revision hash must be 64 hex characters" }
        require(value.removePrefix("sha256:").all { it in '0'..'9' || it in 'a'..'f' }) {
            "revision hash must be lowercase hex"
        }
    }

    override fun toString(): String = value

    fun safeName(): String = value.replace(':', '-')

    companion object {
        fun fromBytes(bytes: ByteArray): Revision {
            val digest = MessageDigest.getInstance("SHA-256").digest(bytes)
            return Revision("sha256:" + digest.joinToString("") { "%02x".format(it) })
        }
    }
}
