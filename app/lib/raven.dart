import 'package:flutter/material.dart';

import 'brand/mark_geometry.dart';
import 'brand/raven_path.dart';
import 'svg_path.dart';
import 'theme/tokens.dart';

/// Иконка Apeiron: ворон в полёте.
///
/// Хугин и Мунин — вороны Одина, облетающие мир и возвращающиеся рассказать
/// увиденное. Гонцы в прямом смысле; ближе к мессенджеру в скандинавском круге
/// образов нет ничего. Имена переводятся как **«мысль»** и **«память»**, что
/// совпадает с расслоением системы: доставка отвечает за «сейчас», архив —
/// за сохранённое.
///
/// **Силуэт не нарисован здесь, а взят готовым.** Шесть итераций ручного
/// подбора контрольных точек безье дали птицу, но не реалистичную: у слепого
/// набора координат низкий потолок. Исходник — CC0 с PhyloPic, происхождение
/// и обоснование в `assets/brand/PROVENANCE.md`.
///
/// **Кривые здесь разрешены.** Это исключение из правила системы: надпись
/// ([ApeironWordmark]) и интерфейсные иконки остаются на прямых с плоскими
/// торцами — руническая резьба, — а птица живая.
class ApeironRaven extends StatelessWidget {
  const ApeironRaven({
    super.key,
    this.size = 64,
    this.color = Ap.bone100,
    this.facingRight = ravenFacesRight,
    this.pitchDegrees = defaultPitch,
    this.inset = 0,
  });

  /// Разворот по умолчанию — обоснование при [ravenDefaultPitch].
  static const double defaultPitch = ravenDefaultPitch;

  final double size;
  final Color color;

  /// Исходный силуэт летит влево. В интерфейсе с письмом слева направо
  /// отправка читается движением вправо, поэтому по умолчанию отражаем.
  final bool facingRight;

  /// Поворот силуэта в градусах, применяется после отражения.
  final double pitchDegrees;

  /// Отступ от краёв поля в логических пикселях.
  final double inset;

  @override
  Widget build(BuildContext context) {
    return SizedBox.square(
      dimension: size,
      child: CustomPaint(
        painter: _RavenPainter(color, facingRight, pitchDegrees, inset),
      ),
    );
  }
}

class _RavenPainter extends CustomPainter {
  const _RavenPainter(this.color, this.facingRight, this.pitch, this.inset);

  final Color color;
  final bool facingRight;
  final double pitch;
  final double inset;

  /// Контур разбирается один раз на весь процесс: разбор строки в четыре
  /// тысячи символов на каждой перерисовке — пустая трата на ровном месте.
  static final Path _source = _buildSource();

  static Path _buildSource() {
    final raw = parseSvgPath(ravenPathData);
    // Вывод potrace перевёрнут по вертикали — возвращаем на место.
    return raw.transform(affine(scaleY: ravenSourceFlipY));
  }

  @override
  void paint(Canvas canvas, Size size) {
    final p = Paint()
      ..color = color
      ..style = PaintingStyle.fill
      ..isAntiAlias = true;
    // Порядок обязателен: сначала зеркало, потом поворот. Зеркало меняет
    // направление вращения на обратное, и поворот до него уводит клюв не туда.
    var shape = _source;
    if (facingRight) shape = mirrorPathX(shape);
    shape = rotatePath(shape, pitch);
    canvas.drawPath(fitPath(shape, Offset.zero & size, inset: inset), p);
  }

  @override
  bool shouldRepaint(_RavenPainter old) =>
      old.color != color ||
      old.facingRight != facingRight ||
      old.pitch != pitch ||
      old.inset != inset;
}
