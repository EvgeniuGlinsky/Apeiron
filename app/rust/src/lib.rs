pub mod api;

#[cfg(not(target_os = "android"))]
mod desktop;

/// Мост, сгенерированный flutter_rust_bridge. Руками не править.
///
/// Строгие линты крейта на него намеренно не распространяются: это машинный
/// код, он переписывается целиком на каждом `flutter_rust_bridge_codegen
/// generate`, и любая правка в нём потеряется молча. Наш код живёт в `api/`,
/// и там запреты на unwrap/expect/panic действуют в полную силу.
#[allow(
    unsafe_code,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
mod frb_generated;
