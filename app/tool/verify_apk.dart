/// Checks whether an APK is fit to hand out.
///
///     cd app && dart run tool/verify_apk.dart [path to APK]
///
/// Without an argument, `build/app/outputs/flutter-apk/app-release.apk` is used.
/// Exit code: 0 — valid, 1 — invalid, 2 — nothing to check.
///
/// Why separate from the build guard in Gradle: that one checks what is being
/// built **now**, and is easy to bypass by building with `--continue` or taking
/// the APK from elsewhere. This one checks **the specific file** that is about
/// to go to a person, and does not care who built it or how.
library;

import 'dart:io';

import 'apk_report.dart';

const _defaultApk = 'build/app/outputs/flutter-apk/app-release.apk';

void main(List<String> args) {
  if (args.length > 1 || args.contains('-h') || args.contains('--help')) {
    stdout.writeln('Usage: dart run tool/verify_apk.dart [path to APK]');
    stdout.writeln('Default: $_defaultApk');
    exitCode = 2;
    return;
  }

  final path = args.isEmpty ? _defaultApk : args.single;
  final file = File(path);
  if (!file.existsSync()) {
    stderr.writeln('No such file: $path');
    exitCode = 2;
    return;
  }

  final ApkContents apk;
  try {
    apk = readApk(file.readAsBytesSync());
  } on FormatException catch (e) {
    stderr.writeln('$path — does not look like an APK: ${e.message}');
    exitCode = 1;
    return;
  }

  final checks = inspect(apk);
  stdout.write(formatReport(path, file.lengthSync(), checks));
  exitCode = checks.any((c) => !c.ok) ? 1 : 0;
}
