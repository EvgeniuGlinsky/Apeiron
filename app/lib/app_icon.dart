import 'package:flutter/material.dart';

import 'brand/mark_geometry.dart';
import 'raven.dart';
import 'theme/tokens.dart';

/// Shape of the icon backplate.
enum IconShell {
  /// Circle — as in Telegram. Neutral and compatible with every system mask.
  circle,

  /// Square with chamfered corners. Closer to our carving, but Android applies
  /// its own mask and eats part of the chamfer.
  chamfered,
}

/// App icon: a coloured backplate with the raven on it.
///
/// Same structure as Telegram's: a solid backplate and a single white
/// silhouette. The technique is proven — on a screen crammed with busy icons,
/// the one with a single shape and a single colour wins.
class ApeironAppIcon extends StatelessWidget {
  const ApeironAppIcon({
    super.key,
    this.size = 128,
    this.background = Ap.basalt900,
    this.glyph = Ap.bone100,
    this.shell = IconShell.circle,
    this.glyphScale = iconGlyphScale,
    this.pitchDegrees = ApeironRaven.defaultPitch,
  });

  /// Rotation of the bird, see [ApeironRaven.pitchDegrees].
  final double pitchDegrees;

  final double size;
  final Color background;
  final Color glyph;
  final IconShell shell;

  /// Share of the field taken by the bird — rationale at [iconGlyphScale].
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
        // Chamfer of 18 % of the side: smaller does not read, larger turns
        // the square into an octagon.
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
