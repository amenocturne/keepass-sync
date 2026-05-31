package space.amenocturne.keepasssync

import android.content.ContentResolver
import android.net.Uri
import android.provider.DocumentsContract

class DocumentTreeRemote(
    private val resolver: ContentResolver,
    private val treeUri: Uri,
) {
    fun read(path: List<String>): ByteArray? {
        val uri = find(path) ?: return null
        return resolver.openInputStream(uri)?.use { it.readBytes() }
    }

    fun write(path: List<String>, bytes: ByteArray) {
        val parent = ensureDirectory(path.dropLast(1))
        val name = path.last()
        findChild(parent.documentId, name)?.let { DocumentsContract.deleteDocument(resolver, it.uri) }
        val file = DocumentsContract.createDocument(resolver, parent.uri, MIME_BINARY, name)
            ?: error("failed to create ${path.joinToString("/")}")
        resolver.openOutputStream(file, "w")?.use { it.write(bytes) }
            ?: error("failed to open ${path.joinToString("/")} for writing")
    }

    fun exists(path: List<String>): Boolean = find(path) != null

    private fun find(path: List<String>): Uri? {
        var current = rootDocument()
        for (segment in path) {
            current = findChild(current.documentId, segment) ?: return null
        }
        return current.uri
    }

    private fun ensureDirectory(path: List<String>): DocumentRef {
        var current = rootDocument()
        for (segment in path) {
            current = findChild(current.documentId, segment)
                ?: createDirectory(current.documentId, segment)
        }
        return current
    }

    private fun rootDocument(): DocumentRef {
        val id = DocumentsContract.getTreeDocumentId(treeUri)
        return DocumentRef(id, DocumentsContract.buildDocumentUriUsingTree(treeUri, id))
    }

    private fun createDirectory(parentId: String, name: String): DocumentRef {
        val parentUri = DocumentsContract.buildDocumentUriUsingTree(treeUri, parentId)
        val uri = DocumentsContract.createDocument(resolver, parentUri, DocumentsContract.Document.MIME_TYPE_DIR, name)
            ?: error("failed to create directory $name")
        return DocumentRef(DocumentsContract.getDocumentId(uri), uri)
    }

    private fun findChild(parentId: String, name: String): DocumentRef? {
        val children = DocumentsContract.buildChildDocumentsUriUsingTree(treeUri, parentId)
        resolver.query(
            children,
            arrayOf(
                DocumentsContract.Document.COLUMN_DOCUMENT_ID,
                DocumentsContract.Document.COLUMN_DISPLAY_NAME,
            ),
            null,
            null,
            null,
        )?.use { cursor ->
            val idColumn = cursor.getColumnIndexOrThrow(DocumentsContract.Document.COLUMN_DOCUMENT_ID)
            val nameColumn = cursor.getColumnIndexOrThrow(DocumentsContract.Document.COLUMN_DISPLAY_NAME)
            while (cursor.moveToNext()) {
                if (cursor.getString(nameColumn) == name) {
                    val id = cursor.getString(idColumn)
                    return DocumentRef(id, DocumentsContract.buildDocumentUriUsingTree(treeUri, id))
                }
            }
        }
        return null
    }

    private data class DocumentRef(val documentId: String, val uri: Uri)

    companion object {
        private const val MIME_BINARY = "application/octet-stream"
    }
}
