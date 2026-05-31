package space.amenocturne.keepasssync

import android.app.Activity
import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.view.ViewGroup
import android.widget.Button
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView

class MainActivity : Activity() {
    private lateinit var prefs: SyncPreferences
    private lateinit var deviceInput: EditText
    private lateinit var localLabel: TextView
    private lateinit var remoteLabel: TextView
    private lateinit var status: TextView

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        prefs = SyncPreferences(this)
        setContentView(buildView())
        refreshLabels()
    }

    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        if (resultCode != RESULT_OK) return
        val uri = data?.data ?: return
        val flags = data.flags and (Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION)
        contentResolver.takePersistableUriPermission(uri, flags)

        when (requestCode) {
            REQUEST_LOCAL -> prefs.localDbUri = uri
            REQUEST_REMOTE -> prefs.remoteRootUri = uri
        }
        refreshLabels()
    }

    private fun buildView(): ScrollView {
        val root = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(32, 32, 32, 32)
            layoutParams = ViewGroup.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.WRAP_CONTENT,
            )
        }

        deviceInput = EditText(this).apply {
            hint = "Device ID"
            setSingleLine(true)
            setText(prefs.deviceId)
        }
        root.addView(deviceInput)

        root.addView(Button(this).apply {
            text = "Choose local KDBX"
            setOnClickListener { chooseLocalDb() }
        })
        localLabel = TextView(this)
        root.addView(localLabel)

        root.addView(Button(this).apply {
            text = "Choose remote folder"
            setOnClickListener { chooseRemoteRoot() }
        })
        remoteLabel = TextView(this)
        root.addView(remoteLabel)

        root.addView(Button(this).apply {
            text = "Sync"
            setOnClickListener { runSync() }
        })

        status = TextView(this).apply {
            textSize = 16f
            setPadding(0, 32, 0, 0)
        }
        root.addView(status)

        return ScrollView(this).apply { addView(root) }
    }

    private fun chooseLocalDb() {
        startActivityForResult(
            Intent(Intent.ACTION_OPEN_DOCUMENT).apply {
                addCategory(Intent.CATEGORY_OPENABLE)
                type = "*/*"
                addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION or Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION)
            },
            REQUEST_LOCAL,
        )
    }

    private fun chooseRemoteRoot() {
        startActivityForResult(
            Intent(Intent.ACTION_OPEN_DOCUMENT_TREE).apply {
                addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION or Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION)
            },
            REQUEST_REMOTE,
        )
    }

    private fun runSync() {
        prefs.deviceId = deviceInput.text.toString().trim().ifBlank { "android" }
        status.text = "Syncing..."
        try {
            val result = AndroidSyncClient(contentResolver, prefs).sync()
            status.text = "${result.action}\n${result.message}"
        } catch (error: Throwable) {
            status.text = "Error: ${error.message}"
        }
        refreshLabels()
    }

    private fun refreshLabels() {
        localLabel.text = "Local: ${prefs.localDbUri ?: "not selected"}"
        remoteLabel.text = "Remote: ${prefs.remoteRootUri ?: "not selected"}"
        status.text = "Base revision: ${prefs.baseRevision ?: "none"}"
    }

    companion object {
        private const val REQUEST_LOCAL = 1
        private const val REQUEST_REMOTE = 2
    }
}
