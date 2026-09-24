/// Parsing an APK and checking that it is fit to hand out.
///
/// Pure functions without `dart:ui` and without Flutter — the same technique
/// as in `android_icon.dart`: a test can call them directly, while
/// `verify_apk.dart` only prints the result and sets the exit code.
///
/// **Why our own zip reading.** Entry names and sizes are needed — and nothing
/// else. Pulling in a dependency for that, with its updates and
/// vulnerabilities, into a project that promises verifiability is a bad trade.
/// Here are two hundred lines that can be read in full. No decompression is
/// needed at all: the zip central directory stores names separately from the
/// contents.
library;

import 'dart:convert';
import 'dart:typed_data';

/// An entry in the archive: name and sizes.
class ZipEntry {
  const ZipEntry(this.name, this.compressedSize, this.uncompressedSize);

  final String name;
  final int compressedSize;
  final int uncompressedSize;
}

/// The APK contents needed for the checks.
class ApkContents {
  const ApkContents({required this.entries, required this.hasSigningBlock});

  /// Central directory entries.
  final List<ZipEntry> entries;

  /// Whether there is an APK signing block (schemes v2/v3).
  ///
  /// Checking by `META-INF/*.RSA` is wrong: that is the v1 scheme signature,
  /// and with `minSdk` 24+ it may be absent altogether although the APK is
  /// signed.
  final bool hasSigningBlock;

  Iterable<String> get names => entries.map((e) => e.name);

  bool has(String name) => names.contains(name);

  Iterable<String> under(String prefix) =>
      names.where((n) => n.startsWith(prefix));
}

/// The magic word at the end of the APK signing block.
const _signingBlockMagic = 'APK Sig Block 42';

const _eocdSignature = 0x06054b50;
const _centralSignature = 0x02014b50;

/// Reads the zip central directory. Entry contents are not touched.
ApkContents readApk(Uint8List bytes) {
  final data = ByteData.sublistView(bytes);

  // The end of central directory is searched from the tail: it may be
  // followed by a comment up to 65535 bytes long.
  var eocd = -1;
  final lowest = bytes.length - 22 - 65535;
  for (var i = bytes.length - 22; i >= (lowest < 0 ? 0 : lowest); i--) {
    if (data.getUint32(i, Endian.little) == _eocdSignature) {
      eocd = i;
      break;
    }
  }
  if (eocd < 0) {
    throw const FormatException(
      'not a zip: end of central directory not found',
    );
  }

  final count = data.getUint16(eocd + 10, Endian.little);
  final directoryOffset = data.getUint32(eocd + 16, Endian.little);
  if (directoryOffset == 0xFFFFFFFF) {
    throw const FormatException(
      'zip64 is not supported: we should never have an APK that large',
    );
  }

  final entries = <ZipEntry>[];
  var at = directoryOffset;
  for (var i = 0; i < count; i++) {
    if (at + 46 > bytes.length ||
        data.getUint32(at, Endian.little) != _centralSignature) {
      throw FormatException('central directory truncated at entry $i');
    }
    final compressed = data.getUint32(at + 20, Endian.little);
    final uncompressed = data.getUint32(at + 24, Endian.little);
    final nameLength = data.getUint16(at + 28, Endian.little);
    final extraLength = data.getUint16(at + 30, Endian.little);
    final commentLength = data.getUint16(at + 32, Endian.little);
    final name = utf8.decode(
      bytes.sublist(at + 46, at + 46 + nameLength),
      allowMalformed: true,
    );
    entries.add(ZipEntry(name, compressed, uncompressed));
    at += 46 + nameLength + extraLength + commentLength;
  }

  // The signing block sits right before the central directory and ends
  // with its magic word.
  var signed = false;
  if (directoryOffset >= 16) {
    final magic = latin1.decode(
      bytes.sublist(directoryOffset - 16, directoryOffset),
      allowInvalid: true,
    );
    signed = magic == _signingBlockMagic;
  }

  return ApkContents(entries: entries, hasSigningBlock: signed);
}

/// Outcome of one check.
class Check {
  const Check(this.ok, this.title, this.detail);

  final bool ok;
  final String title;
  final String detail;
}

/// Name of the native Rust library.
const String rustLibrary = 'librust_lib_apeiron.so';

/// Architectures present in the APK.
List<String> abisOf(ApkContents apk) =>
    apk
        .under('lib/')
        .map((n) => n.split('/'))
        .where((p) => p.length >= 3)
        .map((p) => p[1])
        .toSet()
        .toList()
      ..sort();

/// Full APK check. Returns a list of results — the caller prints them.
List<Check> inspect(ApkContents apk) {
  final checks = <Check>[];
  final abis = abisOf(apk);

  // 1. The Rust library. This check is what it was all about: on 23.09.2026
  //    the build produced an APK without it and reported success.
  if (abis.isEmpty) {
    checks.add(const Check(false, 'native libraries', 'APK has none at all'));
  } else {
    final missing = abis
        .where((abi) => !apk.has('lib/$abi/$rustLibrary'))
        .toList();
    checks.add(
      Check(
        missing.isEmpty,
        'Rust library',
        missing.isEmpty
            ? 'present for ${abis.join(", ")}'
            : 'MISSING for ${missing.join(", ")} (architectures in APK: ${abis.join(", ")})',
      ),
    );
    checks.add(
      Check(
        abis.every((abi) => apk.has('lib/$abi/libflutter.so')),
        'Flutter engine',
        'checked for ${abis.join(", ")}',
      ),
    );
  }

  // 2. Signature. Without it Android refuses to install.
  checks.add(
    Check(
      apk.hasSigningBlock,
      'APK signing',
      apk.hasSigningBlock
          ? 'signing block present'
          : 'no signing block — installation will fail',
    ),
  );

  // 3. App skeleton.
  checks.add(
    Check(
      apk.has('AndroidManifest.xml'),
      'manifest',
      apk.has('AndroidManifest.xml') ? 'present' : 'MISSING',
    ),
  );
  final dex = apk
      .under('')
      .where((n) => RegExp(r'^classes\d*\.dex$').hasMatch(n))
      .length;
  checks.add(Check(dex > 0, 'bytecode', '$dex classes*.dex files'));

  // 4. Icon resources. In release their names are shortened (res/BW.xml), so
  //    we count not names but the fact itself: resource table and files.
  final resFiles = apk.under('res/').length;
  checks.add(
    Check(
      apk.has('resources.arsc') && resFiles >= 5,
      'resources',
      'table ${apk.has("resources.arsc") ? "present" : "MISSING"}, files in res/: $resFiles '
          '(the icon is 5 of them: adaptive, foreground, monochrome, Android 7, launch screen)',
    ),
  );

  // 5. Flutter build. The asset manifest name has changed: now it is
  //    `AssetManifest.bin`, formerly `AssetManifest.json`. We accept either —
  //    checking that the manifest exists, not its format.
  final assets = apk.under('assets/flutter_assets/').length;
  final hasManifest =
      apk.has('assets/flutter_assets/AssetManifest.bin') ||
      apk.has('assets/flutter_assets/AssetManifest.json');
  checks.add(
    Check(
      assets > 0 && hasManifest,
      'Flutter assets',
      hasManifest
          ? '$assets files, asset manifest present'
          : '$assets files, NO ASSET MANIFEST',
    ),
  );

  // 6. Bundled typefaces. `google_fonts` must not be used — it downloads
  //    fonts from Google's servers on first launch, telling them that the app
  //    was installed, along with the user's IP. So the typefaces must live
  //    inside the APK, and they are checked by name: missing even one means
  //    part of the text falls back to the system font.
  final fontFiles = apk
      .under('assets/flutter_assets/assets/fonts/')
      .where((n) => n.endsWith('.otf') || n.endsWith('.ttf'))
      .toList();
  const families = ['Inter', 'Syne', 'JetBrainsMonoNL'];
  final absent = families
      .where((f) => !fontFiles.any((n) => n.split('/').last.startsWith(f)))
      .toList();
  checks.add(
    Check(
      absent.isEmpty && apk.has('assets/flutter_assets/FontManifest.json'),
      'bundled typefaces',
      absent.isEmpty
          ? '${fontFiles.length} files, all three families present'
          : 'MISSING families: ${absent.join(", ")}',
    ),
  );

  return checks;
}

/// Human-readable report.
String formatReport(String path, int sizeBytes, List<Check> checks) {
  final buf = StringBuffer()
    ..writeln('APK:    $path')
    ..writeln('Size:   ${(sizeBytes / (1024 * 1024)).toStringAsFixed(1)} MB')
    ..writeln();
  for (final c in checks) {
    buf.writeln(
      '  ${c.ok ? "[ ok ]" : "[FAIL]"}  ${c.title.padRight(20)} ${c.detail}',
    );
  }
  final bad = checks.where((c) => !c.ok).length;
  buf
    ..writeln()
    ..writeln(
      bad == 0
          ? 'Valid: all checks passed.'
          : 'INVALID: checks failed — $bad. Do not hand it out.',
    );
  return buf.toString();
}
