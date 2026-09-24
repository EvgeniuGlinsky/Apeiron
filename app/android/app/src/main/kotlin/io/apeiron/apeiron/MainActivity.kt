package io.apeiron.apeiron

import android.os.Bundle
import android.view.WindowManager
import io.flutter.embedding.android.FlutterActivity

class MainActivity : FlutterActivity() {
    /**
     * [Vault.register] is called **before** `super.onCreate`, that is, before
     * Flutter brings up the engine and starts Dart.
     *
     * The order matters. `register` loads the native library and gives Rust
     * references to the [Vault] class and the data directory path; without it
     * the very first access to the store would return a refusal.
     *
     * It lets no exceptions out: on failure Rust is left without the
     * references and says so with a clear error. A crash in the Activity
     * constructor, on the other hand, is a black screen without a single line
     * of diagnostics, because the Flutter engine does not exist yet at this
     * point.
     *
     * `FLAG_SECURE` keeps the window out of screenshots, screen recording and
     * the thumbnail in the recent-apps list. Without it the scrambled PIN pad
     * (R-007) would be recorded together with the finger, and the thumbnail
     * would show the last conversation to anyone who opens the list.
     */
    override fun onCreate(savedInstanceState: Bundle?) {
        window.setFlags(
            WindowManager.LayoutParams.FLAG_SECURE,
            WindowManager.LayoutParams.FLAG_SECURE,
        )
        Vault.register(this)
        super.onCreate(savedInstanceState)
    }
}
