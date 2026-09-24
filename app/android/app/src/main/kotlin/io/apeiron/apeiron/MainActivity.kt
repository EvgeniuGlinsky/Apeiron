package io.apeiron.apeiron

import android.os.Bundle
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
     */
    override fun onCreate(savedInstanceState: Bundle?) {
        Vault.register(this)
        super.onCreate(savedInstanceState)
    }
}
