/// Regenerates the Android launcher icon resources.
///
///     cd app && dart run tool/gen_android_icon.dart
///
/// Specifically `dart run`, not `flutter test`: the generator does not touch
/// `dart:ui` and so does not depend on the engine. Run it after any change to
/// the outline (`lib/brand/raven_path.dart`) or to the brand mark numbers
/// (`lib/brand/mark_geometry.dart`).
///
/// A mismatch between what is in the repository and what the generator would
/// produce now is caught by `flutter test test/android_icon_test.dart`.
library;

import 'dart:io';

import 'android_icon.dart';

void main(List<String> args) {
  final root = Directory.current;
  if (!File('${root.path}/pubspec.yaml').existsSync()) {
    stderr.writeln('Run from the app/ directory: pubspec.yaml lives there.');
    exitCode = 2;
    return;
  }

  final files = androidIconFiles();
  for (final entry in files.entries) {
    final out = File('${root.path}/${entry.key}');
    out.parent.createSync(recursive: true);
    out.writeAsStringSync(entry.value);
    stdout.writeln('written  ${entry.key}  (${entry.value.length} B)');
  }

  // Flutter's stock PNGs are its blue logo. While they sit alongside, nothing
  // breaks (the `anydpi` qualifier outranks density ones), but they end up in
  // the build and mislead anyone inspecting the APK.
  for (final d in const ['mdpi', 'hdpi', 'xhdpi', 'xxhdpi', 'xxxhdpi']) {
    final png = File(
      '${root.path}/android/app/src/main/res/mipmap-$d/ic_launcher.png',
    );
    if (png.existsSync()) {
      final dir = png.parent;
      png.deleteSync();
      stdout.writeln('deleted  mipmap-$d/ic_launcher.png');
      if (dir.listSync().isEmpty) dir.deleteSync();
    }
  }
}
