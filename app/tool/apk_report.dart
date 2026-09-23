/// Разбор APK и проверка того, что он годен к выдаче.
///
/// Чистые функции без `dart:ui` и без Flutter — тот же приём, что у
/// `android_icon.dart`: тест может позвать их напрямую, а `verify_apk.dart`
/// только печатает результат и выставляет код возврата.
///
/// **Зачем своё чтение zip.** Нужны имена записей и размеры — и всё. Тянуть
/// ради этого зависимость с её обновлениями и уязвимостями в проект, который
/// обещает проверяемость, — плохой размен. Здесь двести строк, которые можно
/// прочитать целиком. Распаковка не нужна вовсе: центральный каталог zip
/// хранит имена отдельно от содержимого.
library;

import 'dart:convert';
import 'dart:typed_data';

/// Запись в архиве: имя и размеры.
class ZipEntry {
  const ZipEntry(this.name, this.compressedSize, this.uncompressedSize);

  final String name;
  final int compressedSize;
  final int uncompressedSize;
}

/// Содержимое APK, какое нужно для проверок.
class ApkContents {
  const ApkContents({required this.entries, required this.hasSigningBlock});

  /// Записи центрального каталога.
  final List<ZipEntry> entries;

  /// Есть ли блок подписи APK (схемы v2/v3).
  ///
  /// Проверять по `META-INF/*.RSA` нельзя: это подпись схемы v1, и при
  /// `minSdk` 24+ её может не быть вовсе, хотя APK подписан.
  final bool hasSigningBlock;

  Iterable<String> get names => entries.map((e) => e.name);

  bool has(String name) => names.contains(name);

  Iterable<String> under(String prefix) =>
      names.where((n) => n.startsWith(prefix));
}

/// Магическое слово в конце блока подписи APK.
const _signingBlockMagic = 'APK Sig Block 42';

const _eocdSignature = 0x06054b50;
const _centralSignature = 0x02014b50;

/// Читает центральный каталог zip. Содержимое записей не трогается.
ApkContents readApk(Uint8List bytes) {
  final data = ByteData.sublistView(bytes);

  // Конец центрального каталога ищется с хвоста: за ним может стоять
  // комментарий длиной до 65535 байт.
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
      'это не zip: не найден конец центрального каталога',
    );
  }

  final count = data.getUint16(eocd + 10, Endian.little);
  final directoryOffset = data.getUint32(eocd + 16, Endian.little);
  if (directoryOffset == 0xFFFFFFFF) {
    throw const FormatException(
      'zip64 не поддержан: APK такого размера у нас быть не должно',
    );
  }

  final entries = <ZipEntry>[];
  var at = directoryOffset;
  for (var i = 0; i < count; i++) {
    if (at + 46 > bytes.length ||
        data.getUint32(at, Endian.little) != _centralSignature) {
      throw FormatException('центральный каталог оборван на записи $i');
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

  // Блок подписи лежит вплотную перед центральным каталогом и кончается
  // своим магическим словом.
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

/// Итог одной проверки.
class Check {
  const Check(this.ok, this.title, this.detail);

  final bool ok;
  final String title;
  final String detail;
}

/// Имя нативной библиотеки Rust.
const String rustLibrary = 'librust_lib_apeiron.so';

/// Архитектуры, представленные в APK.
List<String> abisOf(ApkContents apk) =>
    apk
        .under('lib/')
        .map((n) => n.split('/'))
        .where((p) => p.length >= 3)
        .map((p) => p[1])
        .toSet()
        .toList()
      ..sort();

/// Полная проверка APK. Возвращает список результатов — печатает вызывающий.
List<Check> inspect(ApkContents apk) {
  final checks = <Check>[];
  final abis = abisOf(apk);

  // 1. Библиотека Rust. Ради этой проверки всё и затевалось: 23.09.2026
  //    сборка выдала APK без неё и отрапортовала успехом.
  if (abis.isEmpty) {
    checks.add(const Check(false, 'нативные библиотеки', 'в APK их нет вовсе'));
  } else {
    final missing = abis
        .where((abi) => !apk.has('lib/$abi/$rustLibrary'))
        .toList();
    checks.add(
      Check(
        missing.isEmpty,
        'библиотека Rust',
        missing.isEmpty
            ? 'на месте для ${abis.join(", ")}'
            : 'НЕТ для ${missing.join(", ")} (архитектуры в APK: ${abis.join(", ")})',
      ),
    );
    checks.add(
      Check(
        abis.every((abi) => apk.has('lib/$abi/libflutter.so')),
        'движок Flutter',
        'проверено для ${abis.join(", ")}',
      ),
    );
  }

  // 2. Подпись. Без неё Android откажется устанавливать.
  checks.add(
    Check(
      apk.hasSigningBlock,
      'подпись APK',
      apk.hasSigningBlock
          ? 'блок подписи на месте'
          : 'блока подписи нет — установка не пройдёт',
    ),
  );

  // 3. Скелет приложения.
  checks.add(
    Check(
      apk.has('AndroidManifest.xml'),
      'манифест',
      apk.has('AndroidManifest.xml') ? 'есть' : 'НЕТ',
    ),
  );
  final dex = apk
      .under('')
      .where((n) => RegExp(r'^classes\d*\.dex$').hasMatch(n))
      .length;
  checks.add(Check(dex > 0, 'байт-код', '$dex файлов classes*.dex'));

  // 4. Ресурсы иконки. В release им укорачивают имена (res/BW.xml), поэтому
  //    считаем не имена, а сам факт: таблица ресурсов и отдельные файлы.
  final resFiles = apk.under('res/').length;
  checks.add(
    Check(
      apk.has('resources.arsc') && resFiles >= 5,
      'ресурсы',
      'таблица ${apk.has("resources.arsc") ? "есть" : "НЕТ"}, файлов в res/: $resFiles '
          '(иконка это 5 из них: адаптивная, передний слой, монохром, Android 7, заставка)',
    ),
  );

  // 5. Сборка Flutter. Имя описи ресурсов менялось: сейчас `AssetManifest.bin`,
  //    раньше был `AssetManifest.json`. Принимаем любое из двух — проверяем
  //    наличие описи, а не её формат.
  final assets = apk.under('assets/flutter_assets/').length;
  final hasManifest =
      apk.has('assets/flutter_assets/AssetManifest.bin') ||
      apk.has('assets/flutter_assets/AssetManifest.json');
  checks.add(
    Check(
      assets > 0 && hasManifest,
      'ресурсы Flutter',
      hasManifest
          ? '$assets файлов, опись на месте'
          : '$assets файлов, ОПИСИ НЕТ',
    ),
  );

  // 6. Вшитые гарнитуры. `google_fonts` использовать нельзя — он скачивает
  //    шрифты с серверов Google при первом запуске, сообщая им факт установки
  //    приложения вместе с IP пользователя. Значит гарнитуры обязаны лежать
  //    внутри APK, и проверяются они поимённо: недостача хотя бы одной
  //    означает, что часть текста поедет системным шрифтом.
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
      'вшитые гарнитуры',
      absent.isEmpty
          ? '${fontFiles.length} файлов, все три семейства на месте'
          : 'НЕТ семейств: ${absent.join(", ")}',
    ),
  );

  return checks;
}

/// Человеческий отчёт.
String formatReport(String path, int sizeBytes, List<Check> checks) {
  final buf = StringBuffer()
    ..writeln('APK:    $path')
    ..writeln('Размер: ${(sizeBytes / (1024 * 1024)).toStringAsFixed(1)} МБ')
    ..writeln();
  for (final c in checks) {
    buf.writeln(
      '  ${c.ok ? "[ да ]" : "[ НЕТ]"}  ${c.title.padRight(20)} ${c.detail}',
    );
  }
  final bad = checks.where((c) => !c.ok).length;
  buf
    ..writeln()
    ..writeln(
      bad == 0
          ? 'Годен: все проверки пройдены.'
          : 'НЕГОДЕН: не пройдено проверок — $bad. Наружу не отдавать.',
    );
  return buf.toString();
}
