import 'dart:io';
import 'dart:ui' as ui;

import 'package:apeiron/mark.dart';
import 'package:apeiron/theme/tokens.dart';
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter_test/flutter_test.dart';

/// Инструмент, а не тест: выкладывает варианты знака на один лист, чтобы их
/// можно было сравнить. Судить логотип по иконке в 22 пикселя нельзя, а
/// выбирать из одного варианта — тем более.
///
///     flutter test test/mark_sheet.dart
///
/// Результат — `build/mark/contact-sheet.png`.
///
/// Имя без суффикса `_test` намеренно: `flutter test` без аргументов подхватывает
/// только `*_test.dart`, а этот файл после записи PNG не отдаёт управление и
/// подвесил бы обычный прогон. По явному пути запускается штатно.
void main() {
  testWidgets('контактный лист знака', (tester) async {
    // Размер тестового окна по умолчанию 800x600 — лист в него не влезает,
    // и переполнение мешает отрисовке. Задаём поверхность явно.
    const sheet = Size(1400, 900);
    tester.view
      ..physicalSize = sheet
      ..devicePixelRatio = 1.0;
    addTearDown(tester.view.reset);

    final key = GlobalKey();
    await tester.pumpWidget(
      MediaQuery(
        data: const MediaQueryData(size: sheet),
        child: Directionality(
          textDirection: TextDirection.ltr,
          child: RepaintBoundary(
            key: key,
            child: const _ContactSheet(),
          ),
        ),
      ),
    );
    await tester.pump(const Duration(milliseconds: 100));

    final boundary =
        key.currentContext!.findRenderObject()! as RenderRepaintBoundary;
    final image = await boundary.toImage(pixelRatio: 1.5);
    final data = await image.toByteData(format: ui.ImageByteFormat.png);
    image.dispose();

    final dir = Directory('build/mark')..createSync(recursive: true);
    final out = File('${dir.path}/contact-sheet.png')
      ..writeAsBytesSync(data!.buffer.asUint8List());

    expect(out.lengthSync(), greaterThan(0));
    // ignore: avoid_print
    print('Контактный лист: ${out.absolute.path}');
  });
}

class _ContactSheet extends StatelessWidget {
  const _ContactSheet();

  static const _labels = {
    MarkVariant.staveAndBranches: 'A · СТОЙКА И ВЕТВИ',
    MarkVariant.brokenStave: 'B · РАЗРЫВ',
    MarkVariant.diverging: 'C · РАСХОЖДЕНИЕ',
  };

  @override
  Widget build(BuildContext context) {
    const label = TextStyle(
      color: Ap.fog400,
      fontSize: 13,
      letterSpacing: 1.6,
      fontWeight: FontWeight.w600,
    );
    const caption = TextStyle(color: Ap.stone600, fontSize: 11);

    return ColoredBox(
      color: Ap.basalt950,
      child: Padding(
        padding: const EdgeInsets.all(32),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            for (final v in MarkVariant.values) ...[
              Text(_labels[v]!, style: label),
              const SizedBox(height: 14),
              Row(
                crossAxisAlignment: CrossAxisAlignment.center,
                children: [
                  // Крупно — для формы. Мелко — проверка на выживание:
                  // знак, который разваливается в 16 пикселей, непригоден.
                  for (final s in [120.0, 64.0, 32.0, 22.0, 16.0])
                    Padding(
                      padding: const EdgeInsets.only(right: 34),
                      child: Column(
                        children: [
                          SizedBox(
                            height: 130,
                            width: 130,
                            child: Center(
                              child: ApeironMark(
                                  size: s, color: Ap.bone100, variant: v),
                            ),
                          ),
                          Text('${s.toInt()} px', style: caption),
                        ],
                      ),
                    ),
                  // На светлом: половина применений — печать и светлая тема.
                  Container(
                    width: 170,
                    height: 130,
                    color: Ap.bone50,
                    alignment: Alignment.center,
                    child: ApeironMark(
                        size: 64, color: Ap.basalt900, variant: v),
                  ),
                  const SizedBox(width: 26),
                  // В акценте — так он стоит в шапке приложения.
                  Container(
                    width: 170,
                    height: 130,
                    color: Ap.basalt800,
                    alignment: Alignment.center,
                    child: ApeironMark(
                        size: 64, color: Ap.glacier400, variant: v),
                  ),
                ],
              ),
              const SizedBox(height: 26),
            ],
          ],
        ),
      ),
    );
  }
}
