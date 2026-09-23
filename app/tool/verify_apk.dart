/// Проверяет, годен ли APK к выдаче наружу.
///
///     cd app && dart run tool/verify_apk.dart [путь к APK]
///
/// Без аргумента берётся `build/app/outputs/flutter-apk/app-release.apk`.
/// Код возврата: 0 — годен, 1 — негоден, 2 — нечего проверять.
///
/// Зачем отдельно от предохранителя в Gradle: тот проверяет то, что собирается
/// **сейчас**, и его легко обойти, собрав с `--continue` или взяв APK из
/// другого места. Этот проверяет **конкретный файл**, который вот-вот уедет
/// человеку, и ему всё равно, кто и как его собрал.
library;

import 'dart:io';

import 'apk_report.dart';

const _defaultApk = 'build/app/outputs/flutter-apk/app-release.apk';

void main(List<String> args) {
  if (args.length > 1 || args.contains('-h') || args.contains('--help')) {
    stdout.writeln('Использование: dart run tool/verify_apk.dart [путь к APK]');
    stdout.writeln('По умолчанию: $_defaultApk');
    exitCode = 2;
    return;
  }

  final path = args.isEmpty ? _defaultApk : args.single;
  final file = File(path);
  if (!file.existsSync()) {
    stderr.writeln('Нет файла: $path');
    exitCode = 2;
    return;
  }

  final ApkContents apk;
  try {
    apk = readApk(file.readAsBytesSync());
  } on FormatException catch (e) {
    stderr.writeln('$path — не похоже на APK: ${e.message}');
    exitCode = 1;
    return;
  }

  final checks = inspect(apk);
  stdout.write(formatReport(path, file.lengthSync(), checks));
  exitCode = checks.any((c) => !c.ok) ? 1 : 0;
}
