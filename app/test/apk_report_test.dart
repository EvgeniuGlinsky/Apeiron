import 'dart:io';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';

import '../tool/apk_report.dart';

/// Test of the APK verifier.
///
/// The verifier exists because on 23.09.2026 the build produced an APK without
/// `librust_lib_apeiron.so` and reported success. This checks that it really
/// rejects such an APK — a guard that has never fired is not a guard.
void main() {
  group('zip reading', () {
    test('garbage is rejected, not half-parsed', () {
      expect(
        () => readApk(Uint8List.fromList(List.filled(1000, 0x41))),
        throwsA(isA<FormatException>()),
      );
    });

    test('an empty file is rejected', () {
      expect(() => readApk(Uint8List(0)), throwsA(isA<FormatException>()));
    });
  });

  group('validity rules', () {
    ApkContents fake(List<String> names, {bool signed = true}) => ApkContents(
      entries: [for (final n in names) ZipEntry(n, 1, 1)],
      hasSigningBlock: signed,
    );

    /// A minimally valid APK — remove items one by one and see if it's caught.
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

    test('the full set is judged valid', () {
      expect(
        passes(fake(goodNames())),
        isTrue,
        reason: failureOf(fake(goodNames())),
      );
    });

    test(
      'an APK without the Rust library is rejected — the whole point of this',
      () {
        final names = goodNames()
          ..remove('lib/arm64-v8a/librust_lib_apeiron.so');
        expect(passes(fake(names)), isFalse);
        expect(failureOf(fake(names)), contains('Rust library'));
      },
    );

    test('library present not for every architecture — also invalid', () {
      final names = goodNames()
        ..addAll(['lib/x86_64/libflutter.so']); // without our library
      expect(passes(fake(names)), isFalse);
      expect(failureOf(fake(names)), contains('Rust library'));
    });

    test('no signature — invalid', () {
      expect(passes(fake(goodNames(), signed: false)), isFalse);
      expect(failureOf(fake(goodNames(), signed: false)), contains('signing'));
    });

    test('a bundled typeface missing — invalid', () {
      final names = goodNames()
        ..remove('assets/flutter_assets/assets/fonts/SyneBold.ttf');
      expect(passes(fake(names)), isFalse);
      expect(failureOf(fake(names)), contains('typefaces'));
    });

    test('no native libraries at all — invalid', () {
      final names = goodNames().where((n) => !n.startsWith('lib/')).toList();
      expect(passes(fake(names)), isFalse);
    });
  });

  test('the real APK, if built, passes the check', () {
    final candidates = [
      'build/app/outputs/flutter-apk/app-release.apk',
      'build/app/outputs/flutter-apk/app-debug.apk',
    ].map(File.new).where((f) => f.existsSync()).toList();

    if (candidates.isEmpty) {
      // We do not force building an APK just to run tests: the build takes
      // minutes, and the verifier logic is tested above on synthetic input.
      markTestSkipped('APK not built — nothing to check');
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
