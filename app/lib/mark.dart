import 'package:flutter/material.dart';

import 'theme/tokens.dart';

/// Варианты знака Apeiron — на выбор, пока не решено.
///
/// Общее у всех трёх и не подлежит пересмотру:
///   * **незамкнутость** — апейрон не имеет границы, поэтому ни колец, ни
///     замкнутых контуров; линии уходят за поле знака;
///   * **только 0°, 45°, 90°**, торцы срезаны плоско — руническая геометрия
///     резьбы по дереву и камню;
///   * **ни одной исторической руны.** Часть скандинавской символики присвоена
///     ультраправыми, а продукт адресован журналистам и правозащитникам.
///     Формы проверяются на совпадение со Старшим Футарком, и совпадения
///     отбраковываются — в частности ᛋ, ᛟ, ᛏ и ᛉ (алгиз: стойка с двумя
///     ветвями вверх, поэтому такая композиция запрещена).
enum MarkVariant {
  /// Стойка с двумя ветвями в противоположных направлениях, длины в отношении
  /// 2 : 3. Ближе всего к первому наброску, но уравновешено и с меньшим вылетом.
  staveAndBranches,

  /// Разорванная стойка: два отрезка с промежутком плюс пересекающая диагональ.
  /// Разрыв и есть высказывание — граница отсутствует буквально.
  brokenStave,

  /// Две параллельные диагонали, которые никогда не сходятся и уходят за поле
  /// с обеих сторон. Самый отвлечённый вариант, руну не напоминает ничем.
  diverging,
}

class ApeironMark extends StatelessWidget {
  const ApeironMark({
    super.key,
    this.size = 64,
    this.color = Ap.stone600,
    this.variant = MarkVariant.staveAndBranches,
  });

  final double size;
  final Color color;
  final MarkVariant variant;

  @override
  Widget build(BuildContext context) {
    return SizedBox.square(
      dimension: size,
      child: CustomPaint(painter: _MarkPainter(color, variant)),
    );
  }
}

class _MarkPainter extends CustomPainter {
  const _MarkPainter(this.color, this.variant);

  final Color color;
  final MarkVariant variant;

  /// Отрезки в долях поля знака. Значения вне [0, 1] — это намеренный вылет
  /// за границу: знак разомкнут. Вылет держится в пределах ~6 %, иначе в
  /// размере 16–24 пикселя от знака остаётся каша.
  static const _segments = <MarkVariant, List<List<double>>>{
    // Стойка + длинная ветвь вверх-вправо + короткая вниз-влево (2 : 3).
    // Обе ветви в разные стороны — это не ᚠ, у которой обе вверх-вправо.
    MarkVariant.staveAndBranches: [
      [0.50, -0.06, 0.50, 1.06],
      [0.50, 0.46, 1.06, -0.10],
      [0.50, 0.62, 0.13, 0.99],
    ],
    // Разорванная стойка + диагональ через разрыв.
    MarkVariant.brokenStave: [
      [0.50, -0.06, 0.50, 0.38],
      [0.50, 0.62, 0.50, 1.06],
      [0.14, 0.86, 0.86, 0.14],
    ],
    // Две параллельные диагонали, не сходящиеся никогда.
    MarkVariant.diverging: [
      [-0.06, 0.62, 0.62, -0.06],
      [0.38, 1.06, 1.06, 0.38],
    ],
  };

  @override
  void paint(Canvas canvas, Size size) {
    final p = Paint()
      ..color = color
      ..strokeWidth = size.shortestSide / 12
      ..strokeCap = StrokeCap.butt
      ..style = PaintingStyle.stroke;

    for (final s in _segments[variant] ?? const <List<double>>[]) {
      canvas.drawLine(
        Offset(s[0] * size.width, s[1] * size.height),
        Offset(s[2] * size.width, s[3] * size.height),
        p,
      );
    }
  }

  @override
  bool shouldRepaint(_MarkPainter old) =>
      old.color != color || old.variant != variant;
}
