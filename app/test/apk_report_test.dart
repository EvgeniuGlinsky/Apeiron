import 'dart:io';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';

import '../tool/apk_report.dart';

/// Проверка верификатора APK.
///
/// Верификатор существует потому, что 23.09.2026 сборка выдала APK без
/// `librust_lib_apeiron.so` и отрапортовала успехом. Здесь проверяется, что он
/// такой APK действительно заворачивает, — предохранитель, который никогда не
/// срабатывал, предохранителем не является.
void main() {
  group('чтение zip', () {
    test('мусор отвергается, а не разбирается наполовину', () {
      expect(
        () => readApk(Uint8List.fromList(List.filled(1000, 0x41))),
        throwsA(isA<FormatException>()),
      );
    });

    test('пустой файл отвергается', () {
      expect(() => readApk(Uint8List(0)), throwsA(isA<FormatException>()));
    });
  });

  group('правила годности', () {
    ApkContents fake(List<String> names, {bool signed = true}) => ApkContents(
      entries: [for (final n in names) ZipEntry(n, 1, 1)],
      hasSigningBlock: signed,
    );

    /// Минимально годный APK — от него отнимаем по одному и смотрим, ловится ли.
    List<String> goodNames() => [
      'AndroidManifest.xml',
      'classes.dex',
      'resources.arsc',
      'res/a.xml',
      'res/b.xml',
      'res/c.xml',
      'res/d.xml',
      'res/e.xml',
      'lib/arm64-v8a/librust_lib_apeiron.so',
      'lib/arm64-v8a/libflutter.so',
      'assets/flutter_assets/AssetManifest.bin',
      'assets/flutter_assets/FontManifest.json',
      'assets/flutter_assets/assets/fonts/Inter-Regular.otf',
      'assets/flutter_assets/assets/fonts/SyneBold.ttf',
      'assets/flutter_assets/assets/fonts/JetBrainsMonoNL-Regular.ttf',
    ];

    bool passes(ApkContents apk) => inspect(apk).every((c) => c.ok);

    String failureOf(ApkContents apk) =>
        inspect(apk).where((c) => !c.ok).map((c) => c.title).join(', ');

    test('полный набор признаётся годным', () {
      expect(
        passes(fake(goodNames())),
        isTrue,
        reason: failureOf(fake(goodNames())),
      );
    });

    test(
      'APK без библиотеки Rust заворачивается — ради этого всё и делалось',
      () {
        final names = goodNames()
          ..remove('lib/arm64-v8a/librust_lib_apeiron.so');
        expect(passes(fake(names)), isFalse);
        expect(failureOf(fake(names)), contains('библиотека Rust'));
      },
    );

    test('библиотека есть не для всех архитектур — тоже негоден', () {
      final names = goodNames()
        ..addAll(['lib/x86_64/libflutter.so']); // без нашей библиотеки
      expect(passes(fake(names)), isFalse);
      expect(failureOf(fake(names)), contains('библиотека Rust'));
    });

    test('без подписи — негоден', () {
      expect(passes(fake(goodNames(), signed: false)), isFalse);
      expect(failureOf(fake(goodNames(), signed: false)), contains('подпись'));
    });

    test('без вшитой гарнитуры — негоден', () {
      final names = goodNames()
        ..remove('assets/flutter_assets/assets/fonts/SyneBold.ttf');
      expect(passes(fake(names)), isFalse);
      expect(failureOf(fake(names)), contains('гарнитуры'));
    });

    test('без нативных библиотек вовсе — негоден', () {
      final names = goodNames().where((n) => !n.startsWith('lib/')).toList();
      expect(passes(fake(names)), isFalse);
    });
  });

  test('настоящий APK, если он собран, проходит проверку', () {
    final candidates = [
      'build/app/outputs/flutter-apk/app-release.apk',
      'build/app/outputs/flutter-apk/app-debug.apk',
    ].map(File.new).where((f) => f.existsSync()).toList();

    if (candidates.isEmpty) {
      // Не заставляем собирать APK ради прогона тестов: сборка занимает минуты,
      // а логика верификатора проверена выше на синтетике.
      markTestSkipped('APK не собран — проверять нечего');
      return;
    }

    for (final apk in candidates) {
      final checks = inspect(readApk(apk.readAsBytesSync()));
      final bad = checks.where((c) => !c.ok).toList();
      expect(
        bad,
        isEmpty,
        reason:
            '${apk.path}: ${bad.map((c) => "${c.title} — ${c.detail}").join("; ")}',
      );
    }
  });
}
