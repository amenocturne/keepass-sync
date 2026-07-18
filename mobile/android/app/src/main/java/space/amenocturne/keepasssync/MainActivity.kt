package space.amenocturne.keepasssync

import android.app.Activity
import android.content.Intent
import android.graphics.Color
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.os.Bundle
import android.text.InputType
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.view.Window
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
        configureWindow()
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
            gravity = Gravity.CENTER_VERTICAL
            setPadding(dp(24), dp(72), dp(24), dp(48))
            layoutParams = ViewGroup.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.MATCH_PARENT,
            )
        }

        deviceInput = EditText(this).apply {
            hint = "Device ID"
            setSingleLine(true)
            setText(prefs.deviceId)
            styleInput()
        }
        root.addView(deviceInput, fieldParams())

        endpointInput = EditText(this).apply {
            hint = "Sync endpoint"
            setSingleLine(true)
            inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_URI
            setText(prefs.endpointUrl)
            styleInput()
        }
        root.addView(endpointInput, fieldParams())

        tokenInput = EditText(this).apply {
            hint = "Sync token"
            setSingleLine(true)
            inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_PASSWORD
            setText(prefs.authToken)
            styleInput()
        }
        root.addView(tokenInput, fieldParams())

        root.addView(Button(this).apply {
            text = "Choose local KDBX"
            stylePrimaryButton()
            setOnClickListener { chooseLocalDb() }
        }, buttonParams())

        localLabel = TextView(this).apply { styleMetaText() }
        root.addView(localLabel, labelParams())

        baseLabel = TextView(this).apply { styleMetaText() }
        root.addView(baseLabel, labelParams())

        root.addView(Button(this).apply {
            text = "Sync"
            stylePrimaryButton()
            setOnClickListener { runSync() }
        }, buttonParams())

        status = TextView(this).apply {
            textSize = 16f
            setTextColor(COLOR_TEXT)
            setLineSpacing(dp(3).toFloat(), 1f)
            gravity = Gravity.CENTER
        }
        root.addView(status, labelParams(topMargin = dp(20)))

        return ScrollView(this).apply {
            setBackgroundColor(COLOR_BACKGROUND)
            isFillViewport = true
            clipToPadding = false
            setOnApplyWindowInsetsListener { view, insets ->
                val top = insets.systemWindowInsetTop + dp(48)
                val bottom = insets.systemWindowInsetBottom + dp(40)
                view.setPadding(0, top, 0, bottom)
                insets
            }
            addView(root)
        }
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

    private fun configureWindow() {
        requestWindowFeature(Window.FEATURE_NO_TITLE)
        window.statusBarColor = COLOR_BACKGROUND
        window.navigationBarColor = COLOR_BACKGROUND
    }

    private fun EditText.styleInput() {
        textSize = 16f
        setTextColor(COLOR_TEXT)
        setHintTextColor(COLOR_MUTED)
        setSingleLine(true)
        background = roundedRect(COLOR_SURFACE, COLOR_STROKE, dp(12))
        setPadding(dp(16), 0, dp(16), 0)
    }

    private fun Button.stylePrimaryButton() {
        textSize = 15f
        setTextColor(COLOR_BUTTON_TEXT)
        typeface = Typeface.DEFAULT_BOLD
        isAllCaps = false
        background = roundedRect(COLOR_ACCENT, COLOR_ACCENT, dp(12))
        minHeight = dp(52)
        minimumHeight = dp(52)
        setPadding(dp(16), 0, dp(16), 0)
    }

    private fun TextView.styleMetaText() {
        textSize = 13f
        setTextColor(COLOR_MUTED)
        gravity = Gravity.CENTER
        setLineSpacing(dp(2).toFloat(), 1f)
    }

    private fun fieldParams(): LinearLayout.LayoutParams =
        LinearLayout.LayoutParams(
            ViewGroup.LayoutParams.MATCH_PARENT,
            dp(54),
        ).apply {
            bottomMargin = dp(12)
        }

    private fun buttonParams(): LinearLayout.LayoutParams =
        LinearLayout.LayoutParams(
            ViewGroup.LayoutParams.MATCH_PARENT,
            dp(52),
        ).apply {
            topMargin = dp(4)
            bottomMargin = dp(12)
        }

    private fun labelParams(topMargin: Int = 0): LinearLayout.LayoutParams =
        LinearLayout.LayoutParams(
            ViewGroup.LayoutParams.MATCH_PARENT,
            ViewGroup.LayoutParams.WRAP_CONTENT,
        ).apply {
            this.topMargin = topMargin
            bottomMargin = dp(12)
        }

    private fun roundedRect(fillColor: Int, strokeColor: Int, radius: Int): GradientDrawable =
        GradientDrawable().apply {
            shape = GradientDrawable.RECTANGLE
            cornerRadius = radius.toFloat()
            setColor(fillColor)
            setStroke(dp(1), strokeColor)
        }

    private fun dp(value: Int): Int = (value * resources.displayMetrics.density).toInt()

    companion object {
        private const val REQUEST_LOCAL = 1
        private val COLOR_BACKGROUND = Color.rgb(16, 20, 24)
        private val COLOR_SURFACE = Color.rgb(27, 34, 40)
        private val COLOR_STROKE = Color.rgb(53, 64, 72)
        private val COLOR_TEXT = Color.rgb(234, 241, 245)
        private val COLOR_MUTED = Color.rgb(151, 164, 174)
        private val COLOR_ACCENT = Color.rgb(125, 211, 252)
        private val COLOR_BUTTON_TEXT = Color.rgb(5, 18, 26)
    }
}
