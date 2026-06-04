package space.amenocturne.keepasssync

import java.net.HttpURLConnection
import java.net.URL
import java.net.URLEncoder

class HttpRemote(
    endpoint: String,
    private val token: String,
) {
    private val baseUrl = endpoint.trim().trimEnd('/')

    init {
        require(baseUrl.startsWith("http://") || baseUrl.startsWith("https://")) {
            "sync endpoint must start with http:// or https://"
        }
        require(token.isNotBlank()) { "sync token is required" }
    }

    fun manifest(): Manifest? {
        val response = request("GET", "/manifest")
        if (response.status == HttpURLConnection.HTTP_NOT_FOUND) return null
        response.requireSuccess()
        return Manifest.parse(response.body.decodeToString())
    }

    fun canonical(): ByteArray {
        val response = request("GET", "/canonical")
        response.requireSuccess()
        return response.body
    }

    fun publish(
        bytes: ByteArray,
        revision: Revision,
        baseRevision: Revision?,
        deviceId: String,
    ): Manifest {
        val query = buildList {
            add("device_id=${encode(deviceId)}")
            add("revision=${encode(revision.value)}")
            if (baseRevision != null) add("base_revision=${encode(baseRevision.value)}")
        }.joinToString("&")
        val response = request("PUT", "/canonical?$query", bytes)
        response.requireSuccess()
        return Manifest.parse(response.body.decodeToString())
    }

    fun preserveIncoming(bytes: ByteArray, revision: Revision, deviceId: String) {
        val response = request(
            "PUT",
            "/incoming/${encode(deviceId)}/${encode(revision.value)}",
            bytes,
        )
        response.requireSuccess()
    }

    private fun request(method: String, path: String, body: ByteArray? = null): HttpResponse {
        val connection = (URL("$baseUrl$path").openConnection() as HttpURLConnection).apply {
            requestMethod = method
            connectTimeout = 15_000
            readTimeout = 30_000
            setRequestProperty("Authorization", "Bearer $token")
            if (body != null) {
                doOutput = true
                setRequestProperty("Content-Type", "application/octet-stream")
            }
        }

        if (body != null) {
            connection.outputStream.use { it.write(body) }
        }

        val status = connection.responseCode
        val stream = if (status in 200..299) connection.inputStream else connection.errorStream
        val responseBody = stream?.use { it.readBytes() } ?: ByteArray(0)
        connection.disconnect()
        return HttpResponse(status, responseBody)
    }

    private fun encode(value: String): String =
        URLEncoder.encode(value, Charsets.UTF_8.name())
}

data class HttpResponse(
    val status: Int,
    val body: ByteArray,
) {
    fun requireSuccess() {
        require(status in 200..299) {
            val text = body.decodeToString().ifBlank { "empty response" }
            "sync server returned HTTP $status: $text"
        }
    }
}
