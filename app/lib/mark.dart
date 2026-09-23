import 'package:flutter/material.dart';

import 'theme/tokens.dart';

/// Знак Apeiron.
///
/// Апейрон — то, что не имеет границы, поэтому знак **незамкнут**: ствол уходит
/// за пределы поля сверху и снизу, ветви обрываются на краю. Ни одного кольца,
/// ни одного замкнутого контура — они бы прямо противоречили имени.
///
/// Геометрия рунная: только 0°, 45° и 90°, торцы срезаны плоско. Но ни одной
/// исторической руны здесь нет и не будет — см. `docs/design.md`, раздел о рунах.
class ApeironMark extends StatelessWidget {
  const ApeironMark({super.key, this.size = 64, this.color = Ap.stone600});

  final double size;
  final Color color;

  @override
  Widget build(BuildContext context) {
    return SizedBox.square(
      dimension: size,
      child: CustomPaint(painter: _MarkPainter(color)),
    );
  }
}

class _MarkPainter extends CustomPainter {
  const _MarkPainter(this.color);

  final Color color;

  @override
  void paint(Canvas canvas, Size size) {
    final p = Paint()
      ..color = color
      ..strokeWidth = size.width / 12
      ..strokeCap = StrokeCap.butt // торцы срезаны плоско
      ..style = PaintingStyle.stroke;

    final cx = size.width / 2;
    final h = size.height;
    final w = size.width;

    // Ствол: выходит за пределы поля с обеих сторон — знак разомкнут.
    canvas.drawLine(Offset(cx, -h * 0.08), Offset(cx, h * 1.08), p);

    // Ветви под 45°, асимметрично. Каждая обрывается на краю, не замыкаясь.
    canvas.drawLine(Offset(cx, h * 0.30), Offset(w * 1.02, h * 0.30 - w * 0.52), p);
    canvas.drawLine(Offset(cx, h * 0.56), Offset(-w * 0.02, h * 0.56 + w * 0.52), p);
    canvas.drawLine(Offset(cx, h * 0.68), Offset(w * 0.86, h * 0.68 - w * 0.36), p);
  }

  @override
  bool shouldRepaint(_MarkPainter old) => old.color != color;
}
