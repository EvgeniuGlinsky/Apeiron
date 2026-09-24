/// Parsing the SVG `d` attribute into a [Path] and transforms on a built path.
///
/// Why our own rather than a package: one function is needed for one brand
/// outline. Pulling in a dependency for that, with its updates and
/// vulnerabilities, is a bad trade in a project that promises verifiability.
/// Here are a hundred lines that can be read in full.
///
/// Number parsing lives in `path_data.dart` — without `dart:ui`, because the
/// same parser is used by the Android icon generator
/// (`tool/gen_android_icon.dart`), which runs outside Flutter. What remains
/// here is the bridge to [Path] and the same transforms, but on an opaque path.
library;

import 'dart:math' as math;
import 'dart:typed_data';
import 'dart:ui';

import 'path_data.dart';

/// Parses the `d` attribute. An unsupported command (`S Q T A`) throws
/// [FormatException] naming it, rather than silently drawing something wrong.
Path parseSvgPath(String d) => buildPath(parsePathData(d));

/// Builds a [Path] from parsed segments.
Path buildPath(List<PathSeg> segs) {
  final path = Path();
  for (final s in segs) {
    switch (s) {
      case MoveSeg(:final x, :final y):
        path.moveTo(x, y);
      case LineSeg(:final x, :final y):
        path.lineTo(x, y);
      case CubicSeg(
        :final x1,
        :final y1,
        :final x2,
        :final y2,
        :final x,
        :final y,
      ):
        path.cubicTo(x1, y1, x2, y2, x, y);
      case CloseSeg():
        path.close();
    }
  }
  return path;
}

/// A planar affine transform in the form [Path.transform] accepts: a 4×4
/// column-major matrix.
///
/// Built by hand so that the utility makes do with `dart:ui` alone and does
/// not pull in vector_math just for scale and translation.
Float64List affine({
  double scaleX = 1,
  double scaleY = 1,
  double translateX = 0,
  double translateY = 0,
}) {
  final m = Float64List(16);
  m[0] = scaleX;
  m[5] = scaleY;
  m[10] = 1;
  m[12] = translateX;
  m[13] = translateY;
  m[15] = 1;
  return m;
}

/// Mirrors the path horizontally relative to its own bounds.
Path mirrorPathX(Path source) {
  final b = source.getBounds();
  return source.transform(affine(scaleX: -1, translateX: b.left + b.right));
}

/// Rotates the path about the centre of its bounds.
///
/// The screen's Y axis points down, so **a positive angle rotates clockwise**,
/// and a negative one counter-clockwise. Rotation should be applied after
/// mirroring: the mirror reverses the direction of rotation.
Path rotatePath(Path source, double degrees) {
  if (degrees == 0) return source;
  final c = source.getBounds().center;
  final r = degrees * math.pi / 180;
  final cos = math.cos(r), sin = math.sin(r);

  final m = Float64List(16);
  m[0] = cos;
  m[1] = sin;
  m[4] = -sin;
  m[5] = cos;
  m[10] = 1;
  m[15] = 1;
  // Translation that puts the centre back after rotating about the origin.
  m[12] = c.dx - (c.dx * cos - c.dy * sin);
  m[13] = c.dy - (c.dx * sin + c.dy * cos);
  return source.transform(m);
}

/// Fits the path into a rectangle, preserving proportions.
///
/// [mirrorX] mirrors horizontally: the source silhouette flies left, while in
/// a left-to-right interface sending reads as movement to the right.
Path fitPath(Path source, Rect box, {bool mirrorX = false, double inset = 0}) {
  final b = source.getBounds();
  if (b.isEmpty) return source;

  final target = box.deflate(inset);
  if (target.isEmpty) return source;

  final k = (target.width / b.width) < (target.height / b.height)
      ? target.width / b.width
      : target.height / b.height;

  final w = b.width * k, h = b.height * k;
  final dx = target.left + (target.width - w) / 2;
  final dy = target.top + (target.height - h) / 2;

  return source.transform(
    affine(
      scaleX: mirrorX ? -k : k,
      scaleY: k,
      translateX: mirrorX ? dx + w + k * b.left : dx - k * b.left,
      translateY: dy - k * b.top,
    ),
  );
}
