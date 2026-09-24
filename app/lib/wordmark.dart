import 'package:flutter/material.dart';

import 'theme/tokens.dart';

/// The APEIRON wordmark: Latin letters drawn by the rules of runic carving.
///
/// The technique: take the ordinary Latin spelling and apply the carving
/// technique to it, rather than substituting runes for letters. The word stays
/// instantly readable but takes on the look of an inscription in stone.
///
/// **The rules all letters are derived from:**
///   * not a single curve — straight segments only;
///   * equal stroke thickness across the whole wordmark;
///   * ends cut flat, with no rounding and no serifs;
///   * horizontals kept to a minimum.
///
/// The last rule is not stylistic. Runes were cut across the wood grain: a
/// horizontal cut ran along the grain, split the blank and was barely
/// visible. Hence the whole look of runic script — stems and diagonals.
/// A horizontal is kept only in `E`, where without it the letter stops reading.
///
/// The strict "only 0°, 45°, 90°" that applies to the icon is deliberately
/// relaxed here: under it `O` would have to be as wide as it is tall, and `E`
/// could not be drawn at all. The carving constraint is more precise and
/// gentler.
class ApeironWordmark extends StatelessWidget {
  const ApeironWordmark({
    super.key,
    this.height = 40,
    this.color = Ap.bone100,
    this.strokeRatio = defaultStrokeRatio,
  });

  /// Capital letter height in logical pixels.
  final double height;

  final Color color;

  /// Stroke thickness as a fraction of letter height. Thinner — the wordmark
  /// falls apart at small sizes; thicker — the counters of `P`, `R` and `O`
  /// fill in.
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

/// Letter outlines.
///
/// Per-letter coordinate system: `x` from zero to the right, `y` from zero (top
/// of the capital) to one (baseline). Each letter is a set of polylines; a
/// polyline is drawn as a continuous line through its points.
abstract final class _Glyphs {
  /// Gap between letters. Large: a spaced-out wordmark reads as more upmarket,
  /// and in carving the letters did stand apart.
  static const double tracking = 0.20;

  static const letters = <_Letter>[
    // A — two diagonals, a chevron crossbar instead of a horizontal.
    // The chevron is deliberately low and wide: higher and narrower, it merges
    // with the apex into a solid triangle and is the first to vanish when
    // scaled down.
    _Letter(0.62, [
      [0.00, 1.00, 0.31, 0.00],
      [0.31, 0.00, 0.62, 1.00],
      [0.10, 0.72, 0.31, 0.51, 0.52, 0.72],
    ]),
    // P — a stem and a triangular flag instead of a half-circle.
    _Letter(0.52, [
      [0.00, 0.00, 0.00, 1.00],
      [0.00, 0.00, 0.52, 0.25, 0.00, 0.50],
    ]),
    // E — the only letter with horizontals: without them it does not read.
    // The middle stroke is shorter than the outer ones, else the letter looks
    // loose.
    _Letter(0.46, [
      [0.00, 0.00, 0.00, 1.00],
      [0.00, 0.00, 0.46, 0.00],
      [0.00, 0.50, 0.34, 0.50],
      [0.00, 1.00, 0.46, 1.00],
    ]),
    // I — a bare stem. The width is not zero: at zero the neighbours E and R
    // close right up to it and "EIR" reads as a clumped lump.
    _Letter(0.06, [
      [0.03, 0.00, 0.03, 1.00],
    ]),
    // R — a flag like P's plus a leg at 45°. The leg starts exactly at the
    // stem: an offset produced a visible notch in the fork.
    _Letter(0.56, [
      [0.00, 0.00, 0.00, 1.00],
      [0.00, 0.00, 0.50, 0.24, 0.00, 0.48],
      [0.00, 0.48, 0.56, 1.00],
    ]),
    // O — a rhombus instead of a circle. Deliberately without spurs at the
    // corners: a rhombus with spurs is ᛟ (othala), and it is in the
    // appropriated set.
    _Letter(0.60, [
      [0.30, 0.00, 0.60, 0.50, 0.30, 1.00, 0.00, 0.50, 0.30, 0.00],
    ]),
    // N — two stems and a diagonal.
    _Letter(0.58, [
      [0.00, 0.00, 0.00, 1.00],
      [0.00, 0.00, 0.58, 1.00],
      [0.58, 0.00, 0.58, 1.00],
    ]),
  ];

  /// Full width of the wordmark as a fraction of letter height.
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

  /// Letter width. For `I` it is zero: the stem itself has no width, the gap
  /// around it comes from tracking.
  final double advance;

  /// Polylines given as a flat list of coordinates `x0, y0, x1, y1, …`.
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
      // Joins at polyline vertices are sharp: a rounded join would betray a
      // casting, not a cut.
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
