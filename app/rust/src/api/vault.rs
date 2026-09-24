//! Мост к хранилищу и аппаратному ключу.
//!
//! Через эту границу проходит только публичное и только описательное. Ключ базы
//! и мастер-ключ наружу не выходят никогда: они живут в Rust, а до него доходят
//! из Kotlin напрямую через JNI, минуя Dart (R-004).

/// Диагностика платформы одной строкой на каждый факт.
///
/// Нужна затем, что проверка на устройстве одна: установка обязана ответить на
/// все вопросы сразу, а не на тот, который догадались задать. Отчёт
/// показывается как есть и пересылается целиком. Секретов не содержит.
#[flutter_rust_bridge::frb]
pub fn platform_diagnostics() -> Result<String, String> {
    let mut report = String::new();
    report.push_str(&format!(
        "SQLite: {}
",
        apeiron_store::sqlite_version()
    ));
    report.push_str(&platform_report());
    Ok(report)
}

#[cfg(target_os = "android")]
fn platform_report() -> String {
    use apeiron_platform::{AndroidVault, KeyWrapper};

    let mut out = String::new();
    match apeiron_platform::storage_dir() {
        Ok(dir) => out.push_str(&format!(
            "каталог данных: {}
",
            dir.display()
        )),
        Err(e) => out.push_str(&format!(
            "каталог данных: {e}
"
        )),
    }
    match AndroidVault.diagnostics() {
        Ok(text) => out.push_str(&text),
        Err(e) => out.push_str(&format!(
            "хранилище ключей: {e}
"
        )),
    }
    out
}

#[cfg(not(target_os = "android"))]
fn platform_report() -> String {
    // Десктоп заморожен решением заказчика, и аппаратного хранилища здесь нет.
    // Молчать об этом нельзя: отсутствие строки читалось бы как «всё в порядке».
    "платформа: не Android, аппаратного хранилища ключей нет
"
    .to_string()
}
