/// Пересобирает ресурсы иконки запуска Android.
///
///     cd app && dart run tool/gen_android_icon.dart
///
/// Именно `dart run`, не `flutter test`: генератор не трогает `dart:ui`
/// и потому не зависит от движка. Запускать после любой правки контура
/// (`lib/brand/raven_path.dart`) или чисел марки (`lib/brand/mark_geometry.dart`).
///
/// Расхождение между тем, что лежит в репозитории, и тем, что выдал бы
/// генератор сейчас, ловит `flutter test test/android_icon_test.dart`.
library;

import 'dart:io';

import 'android_icon.dart';

void main(List<String> args) {
  final root = Directory.current;
  if (!File('${root.path}/pubspec.yaml').existsSync()) {
    stderr.writeln('Запускать из каталога app/: там лежит pubspec.yaml.');
    exitCode = 2;
    return;
  }

  final files = androidIconFiles();
  for (final entry in files.entries) {
    final out = File('${root.path}/${entry.key}');
    out.parent.createSync(recursive: true);
    out.writeAsStringSync(entry.value);
    stdout.writeln('записан  ${entry.key}  (${entry.value.length} Б)');
  }

  // Штатные PNG Flutter — это его синий логотип. Пока они лежат рядом,
  // ничего не ломается (квалификатор `anydpi` старше плотностных), но в
  // сборку они попадают и вводят в заблуждение при разборе APK.
  for (final d in const ['mdpi', 'hdpi', 'xhdpi', 'xxhdpi', 'xxxhdpi']) {
    final png = File(
      '${root.path}/android/app/src/main/res/mipmap-$d/ic_launcher.png',
    );
    if (png.existsSync()) {
      final dir = png.parent;
      png.deleteSync();
      stdout.writeln('удалён   mipmap-$d/ic_launcher.png');
      if (dir.listSync().isEmpty) dir.deleteSync();
    }
  }
}
