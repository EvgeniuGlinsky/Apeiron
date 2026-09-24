import 'dart:io';
import 'dart:math' as math;

import 'package:apeiron/path_data.dart';
import 'package:apeiron/raven.dart';
import 'package:apeiron/svg_path.dart';
import 'package:apeiron/theme/tokens.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import '../tool/android_icon.dart';

/// Test of the Android launcher icon.
///
/// The main thing here is comparing outlines. Both previous approaches to the
/// icon broke on scale: the bird came out the wrong size, and that could only
/// be seen by eye on a device. Now the very [Path] that `ApeironRaven` draws
/// is taken and compared against the outline from the generated `pathData` —
/// by length, bounds and points along the contour.
///
/// There is deliberately no rasterisation here: `toImage()` inside
/// `testWidgets` on this machine keeps the process from exiting (the same
/// pitfall as with the contact sheets). Point comparison is stricter than
/// pixel comparison anyway — it does not depend on anti-aliasing.
void main() {
  test('resources in the repository match the generator output', () {
    for (final entry in androidIconFiles().entries) {
      final f = File(entry.key);
      expect(
        f.existsSync(),
        isTrue,
        reason:
            '${entry.key} is missing — '
            'run dart run tool/gen_android_icon.dart',
      );
      expect(
        f.readAsStringSync().replaceAll('\r\n', '\n'),
        entry.value,
        reason:
            '${entry.key} was edited by hand or is stale; '
            'regenerate: dart run tool/gen_android_icon.dart',
      );
    }
  });

  test('generator colours match the design tokens', () {
    expect(basaltHex, _hex(Ap.basalt900));
    expect(boneHex, _hex(Ap.bone100));
  });

  test('no stock Flutter PNGs are left in the project', () {
    for (final d in const ['mdpi', 'hdpi', 'xhdpi', 'xxhdpi', 'xxxhdpi']) {
      expect(
        File('android/app/src/main/res/mipmap-$d/ic_launcher.png').existsSync(),
        isFalse,
      );
    }
  });

  test('the icon field holds only what it should', () {
    final fore = _iconPathData(foregroundXml());
    expect(
      fore.length,
      1,
      reason: 'the foreground layer is one bird and nothing else',
    );

    final box = boundsOfPathData(parsePathData(fore.single));
    expect(box.left, greaterThanOrEqualTo(0));
    expect(box.top, greaterThanOrEqualTo(0));
    expect(box.right, lessThanOrEqualTo(adaptiveViewport));
    expect(box.bottom, lessThanOrEqualTo(adaptiveViewport));
    // The bird sits at the centre of the field: an offset would break the
    // launcher mask.
    expect(box.centerX, closeTo(adaptiveViewport / 2, 0.02));
    expect(box.centerY, closeTo(adaptiveViewport / 2, 0.02));

    expect(
      _iconPathData(legacyXml()).length,
      2,
      reason: 'the Android 7 icon is the backplate and the bird',
    );
  });

  testWidgets('the foreground layer is the same outline ApeironRaven draws', (
    tester,
  ) async {
    const side = 216.0;

    await tester.pumpWidget(
      const Directionality(
        textDirection: TextDirection.ltr,
        child: Center(
          child: ApeironRaven(size: side, color: Ap.bone100),
        ),
      ),
    );

    final painter = tester
        .widget<CustomPaint>(
          find.descendant(
            of: find.byType(ApeironRaven),
            matching: find.byType(CustomPaint),
          ),
        )
        .painter!;
    final capture = _CapturingCanvas();
    painter.paint(capture, const Size(side, side));
    final drawn = capture.path!;

    // The bird's field in the icon is an iconGlyphScale × 72 square at the
    // centre of 108; in the widget the same square is its whole size. We map
    // one onto the other: if the scale is computed right, the outlines must
    // match. It is the squares that must be related, not the outline boxes:
    // fitting goes by the shorter side, and the box matches the square along
    // one axis only.
    final box = glyphBox(adaptiveMask);
    final k = side / box.width;
    final fromIcon =
        buildPath(
          parsePathData(_iconPathData(foregroundXml()).single),
        ).transform(
          affine(
            scaleX: k,
            scaleY: k,
            translateX: -box.left * k,
            translateY: -box.top * k,
          ),
        );

    // The tolerance comes from rounding on output: coordinates are written
    // with two digits, i.e. to 0.005 of the 108 field, which at this size is
    // 0.02 pixel. Measuring along the contour accumulates as much again. The
    // scale error this was all about would miss by tens of pixels, not by
    // tenths.
    _expectSameOutline(fromIcon, drawn, tolerance: side / 720);
  });

  test('paint does not leave the safe zone', () {
    final path = buildPath(
      parsePathData(_iconPathData(foregroundXml()).single),
    );
    const centre = Offset(adaptiveViewport / 2, adaptiveViewport / 2);

    var worst = 0.0;
    for (final point in _walk(path, step: 0.25)) {
      final r = (point - centre).distance;
      if (r > worst) worst = r;
    }

    // Of the 108 dp field Android promises to show a 66 dp circle; the rest is
    // eaten by the launcher mask. We measure along the contour itself, not the
    // outline box: the box is computed from control points and is always wider
    // than the paint.
    expect(
      worst,
      lessThanOrEqualTo(33.0),
      reason:
          'paint reaches ${worst.toStringAsFixed(2)} dp from the centre '
          'with 33 allowed',
    );
  });
}

// ─── helpers ───────────────────────────────────────────────────────────────

String _hex(Color c) =>
    '#${c.toARGB32().toRadixString(16).toUpperCase().padLeft(8, '0')}';

List<String> _iconPathData(String vectorXml) => RegExp(
  r'android:pathData="([^"]*)"',
).allMatches(vectorXml).map((m) => m.group(1)!).toList();

/// A canvas that draws nothing but remembers the outline.
///
/// `_RavenPainter` is private, and rightly so: what must be checked is not its
/// internals but what it finally puts on the canvas. [noSuchMethod] swallows
/// the other three dozen [Canvas] methods, which are not called here.
class _CapturingCanvas implements Canvas {
  Path? path;

  @override
  void drawPath(Path p, Paint paint) => path = p;

  @override
  dynamic noSuchMethod(Invocation invocation) => null;
}

/// Points along the path contour with spacing [step].
Iterable<Offset> _walk(Path path, {required double step}) sync* {
  for (final metric in path.computeMetrics()) {
    final count = math.max(1, (metric.length / step).ceil());
    for (var i = 0; i <= count; i++) {
      final t = metric.length * i / count;
      final tangent = metric.getTangentForOffset(t);
      if (tangent != null) yield tangent.position;
    }
  }
}

/// Compares two paths along the contour: subpath count, lengths and points.
void _expectSameOutline(Path a, Path b, {required double tolerance}) {
  final ma = a.computeMetrics().toList();
  final mb = b.computeMetrics().toList();
  expect(ma.length, mb.length, reason: 'different number of subpaths');
  expect(ma.isNotEmpty, isTrue);

  for (var i = 0; i < ma.length; i++) {
    expect(
      ma[i].length,
      closeTo(mb[i].length, math.max(0.05, mb[i].length * 0.002)),
      reason: 'subpath $i has a different length',
    );

    const samples = 32;
    for (var j = 0; j <= samples; j++) {
      final pa = ma[i]
          .getTangentForOffset(ma[i].length * j / samples)!
          .position;
      final pb = mb[i]
          .getTangentForOffset(mb[i].length * j / samples)!
          .position;
      expect(
        (pa - pb).distance,
        lessThan(tolerance),
        reason: 'subpath $i, point $j: $pa vs $pb',
      );
    }
  }
}
