/// The `d` attribute as numbers: parsing, transforms, writing back.
///
/// Why separate from `svg_path.dart`: Android accepts **the same syntax** in
/// `android:pathData`, so the launcher icon does not need rasterising — it is
/// enough to repeat the same transforms and write the coordinates back out.
/// For that the path is needed as a list of numbers, not an opaque `Path`, and
/// without `dart:ui`: the icon generator runs under plain `dart run`, outside
/// Flutter.
///
/// `svg_path.dart` is built on top of this file, so there is one parser for
/// all: what the app draws and what goes into the icon are read from one
/// string by one piece of code. They cannot diverge.
///
/// Supported commands are `M m L l H h V v C c Z z` — enough for potrace and
/// Inkscape output. On meeting an unsupported one (`S Q T A`), the parser
/// throws [FormatException] naming the command, rather than silently drawing
/// something wrong.
library;

import 'dart:math' as math;

/// A path segment in absolute coordinates.
sealed class PathSeg {
  const PathSeg();
}

final class MoveSeg extends PathSeg {
  const MoveSeg(this.x, this.y);
  final double x, y;
}

final class LineSeg extends PathSeg {
  const LineSeg(this.x, this.y);
  final double x, y;
}

final class CubicSeg extends PathSeg {
  const CubicSeg(this.x1, this.y1, this.x2, this.y2, this.x, this.y);
  final double x1, y1, x2, y2, x, y;
}

final class CloseSeg extends PathSeg {
  const CloseSeg();
}

final _token = RegExp(
  r'[MmLlHhVvCcSsQqTtAaZz]|[-+]?(?:\d*\.\d+|\d+\.?)(?:[eE][-+]?\d+)?',
);
final _letter = RegExp(r'^[A-Za-z]$');

/// Parses `d` into a list of segments, converting all to absolute coordinates.
List<PathSeg> parsePathData(String d) {
  final t = _token.allMatches(d).map((m) => m[0]!).toList();
  final out = <PathSeg>[];

  var i = 0;
  double cx = 0, cy = 0; // current point
  double sx = 0, sy = 0; // start of the subpath, where Z returns to
  var cmd = '';

  double n() {
    if (i >= t.length) {
      throw const FormatException('path truncated: missing coordinates');
    }
    return double.parse(t[i++]);
  }

  while (i < t.length) {
    if (_letter.hasMatch(t[i])) cmd = t[i++];
    if (i >= t.length && cmd != 'Z' && cmd != 'z') break;

    switch (cmd) {
      case 'M':
        cx = n();
        cy = n();
        out.add(MoveSeg(cx, cy));
        sx = cx;
        sy = cy;
        cmd = 'L'; // repeated coordinates after M mean lines
      case 'm':
        cx += n();
        cy += n();
        out.add(MoveSeg(cx, cy));
        sx = cx;
        sy = cy;
        cmd = 'l';
      case 'L':
        cx = n();
        cy = n();
        out.add(LineSeg(cx, cy));
      case 'l':
        cx += n();
        cy += n();
        out.add(LineSeg(cx, cy));
      case 'H':
        cx = n();
        out.add(LineSeg(cx, cy));
      case 'h':
        cx += n();
        out.add(LineSeg(cx, cy));
      case 'V':
        cy = n();
        out.add(LineSeg(cx, cy));
      case 'v':
        cy += n();
        out.add(LineSeg(cx, cy));
      case 'C':
        final x1 = n(), y1 = n(), x2 = n(), y2 = n();
        cx = n();
        cy = n();
        out.add(CubicSeg(x1, y1, x2, y2, cx, cy));
      case 'c':
        // All six numbers are relative to the point AT THE START of the
        // command, so cx/cy are updated last.
        final x1 = cx + n(), y1 = cy + n();
        final x2 = cx + n(), y2 = cy + n();
        final ex = cx + n(), ey = cy + n();
        out.add(CubicSeg(x1, y1, x2, y2, ex, ey));
        cx = ex;
        cy = ey;
      case 'Z':
      case 'z':
        out.add(const CloseSeg());
        cx = sx;
        cy = sy;
      default:
        throw FormatException('path command "$cmd" is not supported');
    }
  }
  return out;
}

/// A rectangle without `dart:ui`.
class Box {
  const Box(this.left, this.top, this.right, this.bottom);

  /// A square with side [side] centred at ([cx], [cy]).
  factory Box.square(double cx, double cy, double side) =>
      Box(cx - side / 2, cy - side / 2, cx + side / 2, cy + side / 2);

  final double left, top, right, bottom;

  double get width => right - left;
  double get height => bottom - top;
  double get centerX => (left + right) / 2;
  double get centerY => (top + bottom) / 2;
  bool get isEmpty => width <= 0 || height <= 0;

  Box deflate(double d) => Box(left + d, top + d, right - d, bottom - d);

  @override
  String toString() => 'Box($left, $top, $right, $bottom)';
}

/// Path bounds **over all points, including control points**.
///
/// This is exactly how `Path.getBounds()` in `dart:ui` computes them (checked
/// by a test: for the cubic `M0,0 C0,100 100,100 100,0` the height comes out
/// 100, not 75). The match is mandatory: otherwise the icon and what the app
/// draws drift apart in scale — exactly what the previous approach ran into.
Box boundsOfPathData(List<PathSeg> segs) {
  var l = double.infinity, t = double.infinity;
  var r = double.negativeInfinity, b = double.negativeInfinity;
  var seen = false;

  void point(double x, double y) {
    seen = true;
    if (x < l) l = x;
    if (x > r) r = x;
    if (y < t) t = y;
    if (y > b) b = y;
  }

  for (final s in segs) {
    switch (s) {
      case MoveSeg(:final x, :final y):
        point(x, y);
      case LineSeg(:final x, :final y):
        point(x, y);
      case CubicSeg(
        :final x1,
        :final y1,
        :final x2,
        :final y2,
        :final x,
        :final y,
      ):
        point(x1, y1);
        point(x2, y2);
        point(x, y);
      case CloseSeg():
        break;
    }
  }
  return seen ? Box(l, t, r, b) : const Box(0, 0, 0, 0);
}

/// A planar affine transform in SVG `matrix(a b c d e f)` order:
/// `x' = a·x + c·y + e`, `y' = b·x + d·y + f`.
class Aff {
  const Aff(this.a, this.b, this.c, this.d, this.e, this.f);

  const Aff.scale(double sx, double sy) : this(sx, 0, 0, sy, 0, 0);
  const Aff.translate(double tx, double ty) : this(1, 0, 0, 1, tx, ty);

  /// Rotation about the point ([cx], [cy]).
  ///
  /// The Y axis points down, so **a positive angle rotates clockwise**, and a
  /// negative one counter-clockwise.
  factory Aff.rotationAbout(double degrees, double cx, double cy) {
    final r = degrees * math.pi / 180;
    final cos = math.cos(r), sin = math.sin(r);
    return Aff(
      cos,
      sin,
      -sin,
      cos,
      cx - (cx * cos - cy * sin),
      cy - (cx * sin + cy * cos),
    );
  }

  static const identity = Aff(1, 0, 0, 1, 0, 0);

  final double a, b, c, d, e, f;

  /// First `this`, then [next].
  Aff then(Aff next) => Aff(
    next.a * a + next.c * b,
    next.b * a + next.d * b,
    next.a * c + next.c * d,
    next.b * c + next.d * d,
    next.a * e + next.c * f + next.e,
    next.b * e + next.d * f + next.f,
  );

  double mapX(double x, double y) => a * x + c * y + e;
  double mapY(double x, double y) => b * x + d * y + f;
}

List<PathSeg> transformPathData(List<PathSeg> segs, Aff m) => [
  for (final s in segs)
    switch (s) {
      MoveSeg(:final x, :final y) => MoveSeg(m.mapX(x, y), m.mapY(x, y)),
      LineSeg(:final x, :final y) => LineSeg(m.mapX(x, y), m.mapY(x, y)),
      CubicSeg(
        :final x1,
        :final y1,
        :final x2,
        :final y2,
        :final x,
        :final y,
      ) =>
        CubicSeg(
          m.mapX(x1, y1),
          m.mapY(x1, y1),
          m.mapX(x2, y2),
          m.mapY(x2, y2),
          m.mapX(x, y),
          m.mapY(x, y),
        ),
      CloseSeg() => const CloseSeg(),
    },
];

/// Mirrors the path horizontally relative to its own bounds.
/// Twin of `mirrorPathX` from `svg_path.dart`.
List<PathSeg> mirrorDataX(List<PathSeg> segs) {
  final b = boundsOfPathData(segs);
  return transformPathData(segs, Aff(-1, 0, 0, 1, b.left + b.right, 0));
}

/// Rotates the path about the centre of its bounds.
/// Twin of `rotatePath` from `svg_path.dart`.
List<PathSeg> rotateData(List<PathSeg> segs, double degrees) {
  if (degrees == 0) return segs;
  final b = boundsOfPathData(segs);
  return transformPathData(
    segs,
    Aff.rotationAbout(degrees, b.centerX, b.centerY),
  );
}

/// Fits the path into a rectangle, preserving proportions.
/// Twin of `fitPath` from `svg_path.dart`.
List<PathSeg> fitData(List<PathSeg> segs, Box box, {double inset = 0}) {
  final b = boundsOfPathData(segs);
  if (b.isEmpty) return segs;

  final target = box.deflate(inset);
  if (target.isEmpty) return segs;

  final k = (target.width / b.width) < (target.height / b.height)
      ? target.width / b.width
      : target.height / b.height;

  final w = b.width * k, h = b.height * k;
  final dx = target.left + (target.width - w) / 2;
  final dy = target.top + (target.height - h) / 2;

  return transformPathData(
    segs,
    Aff(k, 0, 0, k, dx - k * b.left, dy - k * b.top),
  );
}

/// Writes the segments back into a `d` attribute.
///
/// The command letter is omitted where it repeats — every SVG export does this,
/// and Android's `PathParser` understands it. [precision] digits after the
/// decimal point; in a 108-unit field two are more than enough: 0.01 unit is
/// 0.04 pixel at the highest density.
String formatPathData(List<PathSeg> segs, {int precision = 2}) {
  final buf = StringBuffer();
  var last = '';

  void cmd(String letter, List<double> nums) {
    if (letter != last) {
      if (buf.isNotEmpty) buf.write(' ');
      buf.write(letter);
      last = letter;
    } else {
      buf.write(' ');
    }
    for (var i = 0; i < nums.length; i++) {
      buf.write(i == 0 ? '' : (i.isEven ? ' ' : ','));
      buf.write(_num(nums[i], precision));
    }
  }

  for (final s in segs) {
    switch (s) {
      case MoveSeg(:final x, :final y):
        cmd('M', [x, y]);
      case LineSeg(:final x, :final y):
        cmd('L', [x, y]);
      case CubicSeg(
        :final x1,
        :final y1,
        :final x2,
        :final y2,
        :final x,
        :final y,
      ):
        cmd('C', [x1, y1, x2, y2, x, y]);
      case CloseSeg():
        cmd('Z', const []);
        last = ''; // after Z the next command is written with its letter
    }
  }
  return buf.toString();
}

String _num(double v, int precision) {
  var s = v.toStringAsFixed(precision);
  if (s.contains('.')) {
    s = s.replaceFirst(RegExp(r'0+$'), '');
    s = s.replaceFirst(RegExp(r'\.$'), '');
  }
  return s == '-0' ? '0' : s;
}
