import 'package:flutter/material.dart';

import 'theme/tokens.dart';

/// Логотип-надпись APEIRON: латиница, прорисованная по правилам рунической
/// резьбы.
///
/// Приём: берём обычное латинское написание и накладываем на него технику
/// резьбы, а не подменяем буквы рунами. Слово остаётся мгновенно читаемым, но
/// приобретает облик надписи на камне.
///
/// **Правила, из которых выведены все буквы:**
///   * ни одной кривой — только прямые отрезки;
///   * равная толщина штриха по всей надписи;
///   * торцы срезаны плоско, без скруглений и засечек;
///   * горизонтали сведены к минимуму.
///
/// Последнее правило не стилистическое. Руны резали поперёк волокна дерева:
/// горизонтальный рез шёл вдоль волокна, расщеплял заготовку и был почти не
/// виден. Отсюда и весь облик рунического письма — стойки и диагонали.
/// Горизонталь оставлена только в `E`, где без неё буква перестаёт читаться.
///
/// Строгое «только 0°, 45°, 90°», действующее для иконки, здесь намеренно
/// ослаблено: при нём `O` обязана быть шириной в собственную высоту, а `E`
/// нерисуема вовсе. Ограничение резьбы точнее и мягче.
class ApeironWordmark extends StatelessWidget {
  const ApeironWordmark({
    super.key,
    this.height = 40,
    this.color = Ap.bone100,
    this.strokeRatio = defaultStrokeRatio,
  });

  /// Высота прописной буквы в логических пикселях.
  final double height;

  final Color color;

  /// Толщина штриха в долях высоты буквы. Тоньше — надпись рассыпается в малом
  /// размере, толще — заплывают внутренние просветы `P`, `R` и `O`.
  final double strokeRatio;

  static const double defaultStrokeRatio = 1 / 9;

  @override
  Widget build(BuildContext context) {
    final w = height * _Glyphs.totalAdvance;
    return SizedBox(
      width: w,
      height: height,
      child: CustomPaint(painter: _WordmarkPainter(color, strokeRatio)),
    );
  }
}

/// Контуры букв.
///
/// Система координат на букву: `x` от нуля вправо, `y` от нуля (верх прописной)
/// до единицы (базовая линия). Каждая буква — набор ломаных; ломаная рисуется
/// непрерывной линией по своим точкам.
abstract final class _Glyphs {
  /// Просвет между буквами. Крупный: разрежённая надпись читается дороже,
  /// а в резьбе буквы и стояли отдельно.
  static const double tracking = 0.20;

  static const letters = <_Letter>[
    // A — две диагонали, перекладина шевроном вместо горизонтали.
    // Шеврон намеренно низкий и широкий: выше и уже он сливается с вершиной
    // в сплошной треугольник и пропадает первым при уменьшении.
    _Letter(0.62, [
      [0.00, 1.00, 0.31, 0.00],
      [0.31, 0.00, 0.62, 1.00],
      [0.10, 0.72, 0.31, 0.51, 0.52, 0.72],
    ]),
    // P — стойка и треугольный флаг вместо полукруга.
    _Letter(0.52, [
      [0.00, 0.00, 0.00, 1.00],
      [0.00, 0.00, 0.52, 0.25, 0.00, 0.50],
    ]),
    // E — единственная буква с горизонталями: без них не читается.
    // Средний штрих короче крайних, иначе буква выглядит рыхлой.
    _Letter(0.46, [
      [0.00, 0.00, 0.00, 1.00],
      [0.00, 0.00, 0.46, 0.00],
      [0.00, 0.50, 0.34, 0.50],
      [0.00, 1.00, 0.46, 1.00],
    ]),
    // I — чистая стойка. Ширина не нулевая: при нуле соседи E и R сходятся
    // к ней вплотную и «EIR» читается как слипшийся ком.
    _Letter(0.06, [
      [0.03, 0.00, 0.03, 1.00],
    ]),
    // R — флаг как у P плюс нога под 45°. Нога выходит точно из стойки:
    // отступ давал заметную зарубку в развилке.
    _Letter(0.56, [
      [0.00, 0.00, 0.00, 1.00],
      [0.00, 0.00, 0.50, 0.24, 0.00, 0.48],
      [0.00, 0.48, 0.56, 1.00],
    ]),
    // O — ромб вместо окружности. Намеренно без отростков по углам:
    // ромб с отростками — это ᛟ (отала), а она в присвоенном наборе.
    _Letter(0.60, [
      [0.30, 0.00, 0.60, 0.50, 0.30, 1.00, 0.00, 0.50, 0.30, 0.00],
    ]),
    // N — две стойки и диагональ.
    _Letter(0.58, [
      [0.00, 0.00, 0.00, 1.00],
      [0.00, 0.00, 0.58, 1.00],
      [0.58, 0.00, 0.58, 1.00],
    ]),
  ];

  /// Полная ширина надписи в долях высоты буквы.
  static double get totalAdvance {
    var w = 0.0;
    for (var i = 0; i < letters.length; i++) {
      w += letters[i].advance;
      if (i != letters.length - 1) w += tracking;
    }
    return w;
  }
}

class _Letter {
  const _Letter(this.advance, this.strokes);

  /// Ширина буквы. У `I` она нулевая: сама стойка ширины не имеет, просвет
  /// вокруг неё даёт трекинг.
  final double advance;

  /// Ломаные, заданные плоским списком координат `x0, y0, x1, y1, …`.
  final List<List<double>> strokes;
}

class _WordmarkPainter extends CustomPainter {
  const _WordmarkPainter(this.color, this.strokeRatio);

  final Color color;
  final double strokeRatio;

  @override
  void paint(Canvas canvas, Size size) {
    final h = size.height;
    final p = Paint()
      ..color = color
      ..strokeWidth = h * strokeRatio
      ..strokeCap = StrokeCap.butt
      // Стыки в вершинах ломаных — острые: скруглённый стык выдал бы
      // отливку, а не рез.
      ..strokeJoin = StrokeJoin.miter
      ..style = PaintingStyle.stroke;

    var penX = 0.0;
    for (var i = 0; i < _Glyphs.letters.length; i++) {
      final letter = _Glyphs.letters[i];
      for (final stroke in letter.strokes) {
        final path = Path();
        for (var k = 0; k + 1 < stroke.length; k += 2) {
          final x = (penX + stroke[k]) * h;
          final y = stroke[k + 1] * h;
          if (k == 0) {
            path.moveTo(x, y);
          } else {
            path.lineTo(x, y);
          }
        }
        canvas.drawPath(path, p);
      }
      penX += letter.advance + _Glyphs.tracking;
    }
  }

  @override
  bool shouldRepaint(_WordmarkPainter old) =>
      old.color != color || old.strokeRatio != strokeRatio;
}
