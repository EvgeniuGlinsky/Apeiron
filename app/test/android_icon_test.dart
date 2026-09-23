import 'dart:io';
import 'dart:math' as math;

import 'package:apeiron/path_data.dart';
import 'package:apeiron/raven.dart';
import 'package:apeiron/svg_path.dart';
import 'package:apeiron/theme/tokens.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import '../tool/android_icon.dart';

/// Проверка иконки запуска Android.
///
/// Главное здесь — сравнение контуров. Оба прошлых подхода к иконке ломались
/// на масштабе: птица оказывалась не того размера, и увидеть это можно было
/// только глазами на устройстве. Теперь берётся тот самый [Path], который
/// рисует `ApeironRaven`, и сверяется с контуром из сгенерированного
/// `pathData` — по длине, границам и точкам вдоль обвода.
///
/// Растеризации здесь нет намеренно: `toImage()` внутри `testWidgets` на этой
/// машине не даёт процессу завершиться (та же грабля, что у контактных листов).
/// Сравнение по точкам всё равно строже пиксельного — оно не зависит от
/// сглаживания.
void main() {
  test('ресурсы в репозитории совпадают с выводом генератора', () {
    for (final entry in androidIconFiles().entries) {
      final f = File(entry.key);
      expect(
        f.existsSync(),
        isTrue,
        reason:
            '${entry.key} нет — '
            'запустите dart run tool/gen_android_icon.dart',
      );
      expect(
        f.readAsStringSync().replaceAll('\r\n', '\n'),
        entry.value,
        reason:
            '${entry.key} правлен руками или устарел; '
            'пересоберите: dart run tool/gen_android_icon.dart',
      );
    }
  });

  test('цвета генератора совпадают с токенами оформления', () {
    expect(basaltHex, _hex(Ap.basalt900));
    expect(boneHex, _hex(Ap.bone100));
  });

  test('штатных PNG Flutter в проекте не осталось', () {
    for (final d in const ['mdpi', 'hdpi', 'xhdpi', 'xxhdpi', 'xxxhdpi']) {
      expect(
        File('android/app/src/main/res/mipmap-$d/ic_launcher.png').existsSync(),
        isFalse,
      );
    }
  });

  test('в поле иконки помещается только то, что должно', () {
    final fore = _iconPathData(foregroundXml());
    expect(
      fore.length,
      1,
      reason: 'передний слой — одна птица и ничего больше',
    );

    final box = boundsOfPathData(parsePathData(fore.single));
    expect(box.left, greaterThanOrEqualTo(0));
    expect(box.top, greaterThanOrEqualTo(0));
    expect(box.right, lessThanOrEqualTo(adaptiveViewport));
    expect(box.bottom, lessThanOrEqualTo(adaptiveViewport));
    // Птица стоит по центру поля: смещение сломало бы маску лаунчера.
    expect(box.centerX, closeTo(adaptiveViewport / 2, 0.02));
    expect(box.centerY, closeTo(adaptiveViewport / 2, 0.02));

    expect(
      _iconPathData(legacyXml()).length,
      2,
      reason: 'иконка для Android 7 — подложка и птица',
    );
  });

  testWidgets('передний слой — тот же контур, что рисует ApeironRaven', (
    tester,
  ) async {
    const side = 216.0;

    await tester.pumpWidget(
      const Directionality(
        textDirection: TextDirection.ltr,
        child: Center(
          child: ApeironRaven(size: side, color: Ap.bone100),
        ),
      ),
    );

    final painter = tester
        .widget<CustomPaint>(
          find.descendant(
            of: find.byType(ApeironRaven),
            matching: find.byType(CustomPaint),
          ),
        )
        .painter!;
    final capture = _CapturingCanvas();
    painter.paint(capture, const Size(side, side));
    final drawn = capture.path!;

    // Поле птицы в иконке — квадрат iconGlyphScale × 72 по центру 108;
    // в виджете тот же квадрат равен всему его размеру. Растягиваем одно
    // на другое: если масштаб посчитан верно, контуры обязаны совпасть.
    // Соотносить надо именно квадраты, а не рамки контура: вписывание идёт
    // по меньшей стороне, и рамка совпадает с квадратом только по одной оси.
    final box = glyphBox(adaptiveMask);
    final k = side / box.width;
    final fromIcon =
        buildPath(
          parsePathData(_iconPathData(foregroundXml()).single),
        ).transform(
          affine(
            scaleX: k,
            scaleY: k,
            translateX: -box.left * k,
            translateY: -box.top * k,
          ),
        );

    // Допуск — от округления записи: координаты пишутся с двумя знаками,
    // то есть с точностью 0,005 поля 108, что на этом размере даёт 0,02
    // пикселя. Измерение вдоль обвода накапливает ещё столько же. Ошибка
    // масштаба, ради которой всё это и затевалось, промахнулась бы на
    // десятки пикселей, а не на десятые.
    _expectSameOutline(fromIcon, drawn, tolerance: side / 720);
  });

  test('краска не выходит из безопасной зоны', () {
    final path = buildPath(
      parsePathData(_iconPathData(foregroundXml()).single),
    );
    const centre = Offset(adaptiveViewport / 2, adaptiveViewport / 2);

    var worst = 0.0;
    for (final point in _walk(path, step: 0.25)) {
      final r = (point - centre).distance;
      if (r > worst) worst = r;
    }

    // Из поля 108 dp Android обещает показать круг 66 dp, остальное съедает
    // маска лаунчера. Меряем по самому обводу, а не по рамке контура:
    // рамка считается по контрольным точкам и всегда шире краски.
    expect(
      worst,
      lessThanOrEqualTo(33.0),
      reason:
          'краска уходит на ${worst.toStringAsFixed(2)} dp от центра '
          'при допустимых 33',
    );
  });
}

// ─── вспомогательное ───────────────────────────────────────────────────────

String _hex(Color c) =>
    '#${c.toARGB32().toRadixString(16).toUpperCase().padLeft(8, '0')}';

List<String> _iconPathData(String vectorXml) => RegExp(
  r'android:pathData="([^"]*)"',
).allMatches(vectorXml).map((m) => m.group(1)!).toList();

/// Холст, который ничего не рисует, а запоминает контур.
///
/// `_RavenPainter` закрыт, и это правильно: проверять надо не его внутренности,
/// а то, что он в итоге кладёт на холст. [noSuchMethod] гасит остальные три
/// десятка методов [Canvas], которые здесь не вызываются.
class _CapturingCanvas implements Canvas {
  Path? path;

  @override
  void drawPath(Path p, Paint paint) => path = p;

  @override
  dynamic noSuchMethod(Invocation invocation) => null;
}

/// Точки вдоль обвода пути с шагом [step].
Iterable<Offset> _walk(Path path, {required double step}) sync* {
  for (final metric in path.computeMetrics()) {
    final count = math.max(1, (metric.length / step).ceil());
    for (var i = 0; i <= count; i++) {
      final t = metric.length * i / count;
      final tangent = metric.getTangentForOffset(t);
      if (tangent != null) yield tangent.position;
    }
  }
}

/// Сверяет два пути по обводу: число подконтуров, их длины и точки на них.
void _expectSameOutline(Path a, Path b, {required double tolerance}) {
  final ma = a.computeMetrics().toList();
  final mb = b.computeMetrics().toList();
  expect(ma.length, mb.length, reason: 'разное число подконтуров');
  expect(ma.isNotEmpty, isTrue);

  for (var i = 0; i < ma.length; i++) {
    expect(
      ma[i].length,
      closeTo(mb[i].length, math.max(0.05, mb[i].length * 0.002)),
      reason: 'подконтур $i другой длины',
    );

    const samples = 32;
    for (var j = 0; j <= samples; j++) {
      final pa = ma[i]
          .getTangentForOffset(ma[i].length * j / samples)!
          .position;
      final pb = mb[i]
          .getTangentForOffset(mb[i].length * j / samples)!
          .position;
      expect(
        (pa - pb).distance,
        lessThan(tolerance),
        reason: 'подконтур $i, точка $j: $pa против $pb',
      );
    }
  }
}
