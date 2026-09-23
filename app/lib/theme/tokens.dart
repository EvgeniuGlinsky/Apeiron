import 'package:flutter/material.dart';

/// Токены оформления Apeiron. Спецификация — `docs/design.md`.
///
/// Единственное жёсткое правило графики: 0°, 45°, 90°. Никаких скруглений.
/// Поэтому [Ap.radius] здесь нет — углы всюду прямые, и это намеренно.
abstract final class Ap {
  // Базальт — холодный почти-чёрный с синим подтоном.
  static const basalt950 = Color(0xFF0B0E11);
  static const basalt900 = Color(0xFF12161A);
  static const basalt800 = Color(0xFF1A2026);

  // Камень — границы и разделители.
  static const stone700 = Color(0xFF2A333B);
  static const stone600 = Color(0xFF3A444D);

  // Туман и кость. Основной текст намеренно НЕ белый: чистый белый на тёмном
  // режет глаз и выглядит дёшево, тёплая кость читается как берёза.
  static const fog400 = Color(0xFF8A97A3);
  static const bone100 = Color(0xFFE8E6E1);
  static const bone50 = Color(0xFFF4F2ED);

  /// Ледниковый — обычный акцент. Ненасыщенный: насыщенные акценты удешевляют.
  static const glacier400 = Color(0xFF8FB3C9);
  static const glacier600 = Color(0xFF4A7290);

  /// Медь — только состояния сверки ключей. Не золото: бронза настоящий
  /// материал этой культуры и читается дороже именно потому, что тише.
  static const ember400 = Color(0xFFC77B52);

  /// Ржавчина — тревога. Приглушена: тревога должна быть заметна, не кричать.
  static const rust500 = Color(0xFFB4543A);

  // Шаг компоновки. Базовая единица 4, шаг 8.
  static const s4 = 4.0;
  static const s8 = 8.0;
  static const s12 = 12.0;
  static const s16 = 16.0;
  static const s20 = 20.0;
  static const s28 = 28.0;
  static const s40 = 40.0;

  /// Гарнитуры. Файлы лежат в `assets/fonts/`, объявлены в `pubspec.yaml`.
  ///
  /// ВАЖНО: пакет `google_fonts` использовать нельзя — он скачивает шрифты
  /// с fonts.gstatic.com при первом запуске, сообщая Google факт установки
  /// приложения с IP пользователя. Только бандл.
  static const String uiFont = 'Inter';
  static const String displayFont = 'Syne';
  static const monoFallback = <String>[
    'JetBrains Mono',
    'Cascadia Mono',
    'Consolas',
    'DejaVu Sans Mono',
    'monospace',
  ];

  static ThemeData dark() {
    const scheme = ColorScheme.dark(
      surface: basalt900,
      onSurface: bone100,
      primary: glacier400,
      onPrimary: basalt950,
      secondary: ember400,
      onSecondary: basalt950,
      error: rust500,
      onError: bone50,
      outline: stone600,
      outlineVariant: stone700,
    );

    return ThemeData(
      useMaterial3: true,
      colorScheme: scheme,
      scaffoldBackgroundColor: basalt950,
      fontFamily: uiFont,
      // Прямые углы во всех компонентах — правило системы, не вкусовщина.
      cardTheme: const CardThemeData(
        color: basalt800,
        elevation: 0,
        margin: EdgeInsets.zero,
        shape: RoundedRectangleBorder(
          side: BorderSide(color: stone700),
          borderRadius: BorderRadius.zero,
        ),
      ),
      filledButtonTheme: FilledButtonThemeData(
        style: FilledButton.styleFrom(
          shape: const RoundedRectangleBorder(borderRadius: BorderRadius.zero),
          padding: const EdgeInsets.symmetric(horizontal: s28, vertical: s16),
          textStyle: const TextStyle(
            fontSize: 14,
            fontWeight: FontWeight.w600,
            letterSpacing: 0.8,
          ),
        ),
      ),
      outlinedButtonTheme: OutlinedButtonThemeData(
        style: OutlinedButton.styleFrom(
          shape: const RoundedRectangleBorder(borderRadius: BorderRadius.zero),
          padding: const EdgeInsets.symmetric(horizontal: s20, vertical: s12),
        ),
      ),
      snackBarTheme: const SnackBarThemeData(
        backgroundColor: basalt800,
        contentTextStyle: TextStyle(color: bone100),
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.zero),
        behavior: SnackBarBehavior.floating,
      ),
      appBarTheme: const AppBarTheme(
        backgroundColor: basalt950,
        surfaceTintColor: Colors.transparent,
        elevation: 0,
        centerTitle: false,
        titleTextStyle: TextStyle(
          color: bone100,
          fontSize: 20,
          fontWeight: FontWeight.w600,
          letterSpacing: 0.4,
        ),
      ),
      dividerTheme: const DividerThemeData(color: stone700, thickness: 1),
      textTheme: const TextTheme(
        displaySmall: TextStyle(
          fontSize: 40,
          height: 1.2,
          letterSpacing: 0.8,
          fontWeight: FontWeight.w600,
          color: bone100,
        ),
        titleLarge: TextStyle(
          fontSize: 20,
          height: 1.2,
          letterSpacing: 0.4,
          fontWeight: FontWeight.w600,
          color: bone100,
        ),
        bodyMedium: TextStyle(fontSize: 14, height: 1.5, color: bone100),
        bodySmall: TextStyle(fontSize: 13, height: 1.5, color: fog400),
        labelLarge: TextStyle(
          fontSize: 12,
          height: 1.4,
          letterSpacing: 1.0,
          fontWeight: FontWeight.w600,
          color: fog400,
        ),
        labelMedium: TextStyle(
          fontSize: 12,
          height: 1.4,
          letterSpacing: 0.8,
          color: fog400,
        ),
      ),
    );
  }

  /// Моноширинный стиль для отпечатков, ключей и кодов.
  ///
  /// Разборчивость здесь — вопрос безопасности: путаница `0`/`O` или `1`/`l`
  /// в числе сверки означает пропущенного посредника.
  static TextStyle mono({
    double size = 13,
    Color color = bone100,
    double spacing = 0,
    FontWeight weight = FontWeight.w400,
  }) =>
      TextStyle(
        fontFamily: monoFallback.first,
        fontFamilyFallback: monoFallback.sublist(1),
        fontSize: size,
        color: color,
        letterSpacing: spacing,
        fontWeight: weight,
        height: 1.3,
      );
}
