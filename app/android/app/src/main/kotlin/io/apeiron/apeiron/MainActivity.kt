package io.apeiron.apeiron

import android.os.Bundle
import io.flutter.embedding.android.FlutterActivity

class MainActivity : FlutterActivity() {
    /**
     * [Vault.register] вызывается **до** `super.onCreate`, то есть до того, как
     * Flutter поднимет движок и запустит Dart.
     *
     * Порядок существенный. `register` грузит нативную библиотеку и отдаёт Rust
     * ссылки на класс [Vault] и путь к каталогу данных; без этого первое же
     * обращение к хранилищу вернуло бы отказ.
     *
     * Исключений отсюда не выпускает: при неудаче Rust останется без ссылок и
     * скажет об этом внятной ошибкой. Падение же в конструкторе Activity — это
     * чёрный экран без единой строчки диагностики, потому что Flutter-движка к
     * этому моменту ещё нет.
     */
    override fun onCreate(savedInstanceState: Bundle?) {
        Vault.register(this)
        super.onCreate(savedInstanceState)
    }
}
