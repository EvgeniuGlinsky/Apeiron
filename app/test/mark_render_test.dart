import 'dart:io';
import 'dart:ui' as ui;

import 'package:apeiron/mark.dart';
import 'package:apeiron/theme/tokens.dart';
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter_test/flutter_test.dart';

/// Выгружает знак в PNG настоящим кодом отрисовки, чтобы его можно было
/// рассмотреть в большом размере.
///
/// Это не обычный тест, а инструмент: судить логотип по иконке в 22 пикселя
/// в шапке нельзя. Запуск:
///
///     flutter test test/mark_render_test.dart
///
/// Результат ложится в `build/mark/`.
void main() {
  testWidgets('выгрузка знака в PNG', (tester) async {
    final dir = Directory('build/mark')..createSync(recursive: true);

    final variants = <String, ({Color bg, Color fg, double size})>{
      'mark-dark-512': (bg: Ap.basalt950, fg: Ap.bone100, size: 512),
      'mark-dark-accent-512': (bg: Ap.basalt950, fg: Ap.glacier400, size: 512),
      'mark-light-512': (bg: Ap.bone50, fg: Ap.basalt900, size: 512),
      'mark-dark-96': (bg: Ap.basalt950, fg: Ap.bone100, size: 96),
      'mark-dark-32': (bg: Ap.basalt950, fg: Ap.bone100, size: 32),
      'mark-dark-16': (bg: Ap.basalt950, fg: Ap.bone100, size: 16),
    };

    for (final entry in variants.entries) {
      final v = entry.value;
      final key = GlobalKey();
      // Поле в полтора размера знака: знак разомкнут и выходит за свои границы,
      // поэтому рисовать его впритык нельзя — обрежется.
      final canvasSize = v.size * 1.5;

      await tester.pumpWidget(
        MediaQuery(
          data: const MediaQueryData(),
          child: Directionality(
            textDirection: TextDirection.ltr,
            child: Center(
              child: RepaintBoundary(
                key: key,
                child: Container(
                  width: canvasSize,
                  height: canvasSize,
                  color: v.bg,
                  alignment: Alignment.center,
                  child: ApeironMark(size: v.size, color: v.fg),
                ),
              ),
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();

      final boundary =
          key.currentContext!.findRenderObject()! as RenderRepaintBoundary;
      // Мелкие размеры выгружаем с увеличением, иначе на них нечего смотреть.
      final ratio = v.size >= 256 ? 2.0 : 8.0;
      final image = await boundary.toImage(pixelRatio: ratio);
      final data = await image.toByteData(format: ui.ImageByteFormat.png);
      File('${dir.path}/${entry.key}.png')
          .writeAsBytesSync(data!.buffer.asUint8List());
      image.dispose();
    }

    final written = dir.listSync().whereType<File>().length;
    expect(written, variants.length);
    // ignore: avoid_print
    print('Знак выгружен в ${dir.absolute.path} — $written файлов');
  });
}
