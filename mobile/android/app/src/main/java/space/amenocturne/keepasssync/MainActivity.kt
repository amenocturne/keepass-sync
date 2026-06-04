package space.amenocturne.keepasssync

import android.app.Activity
import android.content.Intent
import android.os.Bundle
import android.text.InputType
import android.view.ViewGroup
import android.widget.Button
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView

class MainActivity : Activity() {
    private lateinit var prefs: SyncPreferences
    private lateinit var deviceInput: EditText
    private lateinit var endpointInput: EditText
    private lateinit var tokenInput: EditText
    private lateinit var localLabel: TextView
    private lateinit var baseLabel: TextView
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

        endpointInput = EditText(this).apply {
            hint = "Sync endpoint"
            setSingleLine(true)
            inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_URI
            setText(prefs.endpointUrl)
        }
        root.addView(endpointInput)

        tokenInput = EditText(this).apply {
            hint = "Sync token"
            setSingleLine(true)
            inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_PASSWORD
            setText(prefs.authToken)
        }
        root.addView(tokenInput)

        root.addView(Button(this).apply {
            text = "Choose local KDBX"
            setOnClickListener { chooseLocalDb() }
        })
        localLabel = TextView(this)
        root.addView(localLabel)

        baseLabel = TextView(this)
        root.addView(baseLabel)

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

    private fun runSync() {
        prefs.deviceId = deviceInput.text.toString().trim().ifBlank { "android" }
        prefs.endpointUrl = endpointInput.text.toString().trim()
        prefs.authToken = tokenInput.text.toString()
        status.text = "Syncing..."
        try {
            val result = AndroidSyncClient(contentResolver, prefs).sync()
            refreshLabels()
            status.text = "${result.action}\n${result.message}"
        } catch (error: Throwable) {
            status.text = "Error: ${error.message}"
        }
    }

    private fun refreshLabels() {
        localLabel.text = "Local: ${prefs.localDbUri ?: "not selected"}"
        baseLabel.text = "Base revision: ${prefs.baseRevision ?: "none"}"
    }

    companion object {
        private const val REQUEST_LOCAL = 1
    }
}
