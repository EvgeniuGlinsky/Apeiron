import 'package:flutter/material.dart';

import 'raven.dart';
import 'theme/tokens.dart';

/// Форма подложки иконки.
enum IconShell {
  /// Круг — как у Telegram. Нейтрально и совместимо со всеми масками системы.
  circle,

  /// Квадрат со срезанными углами. Ближе нашей резьбе, но Android наложит
  /// собственную маску и часть среза съест.
  chamfered,
}

/// Иконка приложения: цветная подложка и ворон на ней.
///
/// Устройство то же, что у Telegram: сплошная подложка и один белый силуэт.
/// Приём проверен — на экране, забитом пёстрыми иконками, выигрывает та,
/// в которой одна форма и один цвет.
class ApeironAppIcon extends StatelessWidget {
  const ApeironAppIcon({
    super.key,
    this.size = 128,
    this.background = Ap.basalt900,
    this.glyph = Ap.bone100,
    this.shell = IconShell.circle,
    this.glyphScale = 0.70,
    this.pitchDegrees = ApeironRaven.defaultPitch,
  });

  /// Разворот птицы, см. [ApeironRaven.pitchDegrees].
  final double pitchDegrees;

  final double size;
  final Color background;
  final Color glyph;
  final IconShell shell;

  /// Доля поля, которую занимает птица. Меньше 0,55 — иконка выглядит пустой,
  /// больше 0,7 — силуэт упирается в края и теряет очертания.
  final double glyphScale;

  @override
  Widget build(BuildContext context) {
    return SizedBox.square(
      dimension: size,
      child: Stack(
        alignment: Alignment.center,
        children: [
          Positioned.fill(
            child: CustomPaint(painter: _ShellPainter(background, shell)),
          ),
          ApeironRaven(
            size: size * glyphScale,
            color: glyph,
            pitchDegrees: pitchDegrees,
          ),
        ],
      ),
    );
  }
}

class _ShellPainter extends CustomPainter {
  const _ShellPainter(this.color, this.shell);

  final Color color;
  final IconShell shell;

  @override
  void paint(Canvas canvas, Size size) {
    final p = Paint()
      ..color = color
      ..style = PaintingStyle.fill
      ..isAntiAlias = true;

    switch (shell) {
      case IconShell.circle:
        canvas.drawCircle(
          Offset(size.width / 2, size.height / 2),
          size.shortestSide / 2,
          p,
        );

      case IconShell.chamfered:
        // Срез в 18 % стороны: мельче не читается, крупнее превращает
        // квадрат в восьмиугольник.
        final c = size.shortestSide * 0.18;
        final w = size.width, h = size.height;
        final path = Path()
          ..moveTo(c, 0)
          ..lineTo(w - c, 0)
          ..lineTo(w, c)
          ..lineTo(w, h - c)
          ..lineTo(w - c, h)
          ..lineTo(c, h)
          ..lineTo(0, h - c)
          ..lineTo(0, c)
          ..close();
        canvas.drawPath(path, p);
    }
  }

  @override
  bool shouldRepaint(_ShellPainter old) =>
      old.color != color || old.shell != shell;
}
