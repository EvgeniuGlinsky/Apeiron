import 'dart:io';
import 'dart:ui' as ui;

import 'package:apeiron/path_data.dart';
import 'package:apeiron/svg_path.dart';
import 'package:apeiron/theme/tokens.dart';
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart' show FontLoader, rootBundle;
import 'package:flutter_test/flutter_test.dart';

import '../tool/android_icon.dart';

/// A tool, not a test: shows the launcher icon the way the system will show it.
///
///     flutter test test/android_icon_sheet.dart
///
/// Output — `build/mark/android-icon-sheet.png`.
/// The name lacks the `_test` suffix on purpose — see `wordmark_sheet.dart`.
/// The process will not exit: wait for the file to appear, not for the exit.
///
/// What is drawn is **the generated `pathData`**, the very one that goes into
/// the APK — not the widget. Otherwise the sheet would show something other
/// than what the user will see.
void main() {
  testWidgets('Android icon sheet', (tester) async {
    await (FontLoader(
      'Inter',
    )..addFont(rootBundle.load('assets/fonts/Inter-SemiBold.otf'))).load();

    const sheet = Size(1460, 1000);
    tester.view
      ..physicalSize = sheet
      ..devicePixelRatio = 1.0;
    addTearDown(tester.view.reset);

    final key = GlobalKey();
    await tester.pumpWidget(
      MediaQuery(
        data: const MediaQueryData(size: sheet),
        child: Directionality(
          textDirection: TextDirection.ltr,
          child: RepaintBoundary(key: key, child: const _Sheet()),
        ),
      ),
    );
    await tester.pump(const Duration(milliseconds: 100));

    final boundary =
        key.currentContext!.findRenderObject()! as RenderRepaintBoundary;
    final image = await boundary.toImage(pixelRatio: 1.5);
    final data = await image.toByteData(format: ui.ImageByteFormat.png);
    image.dispose();

    final dir = Directory('build/mark')..createSync(recursive: true);
    final out = File('${dir.path}/android-icon-sheet.png')
      ..writeAsBytesSync(data!.buffer.asUint8List());

    expect(out.lengthSync(), greaterThan(0));
    // ignore: avoid_print
    print('Android icon sheet: ${out.absolute.path}');
  });
}

/// Masks launchers use to cut the adaptive icon.
enum _Mask { circle, squircle, rounded, square }

Path _maskPath(_Mask mask, Size s) {
  final r = Offset.zero & s;
  return switch (mask) {
    _Mask.circle => Path()..addOval(r),
    // The Pixel launcher's superellipse is approximated by rounding at 28 %
    // of the side.
    _Mask.squircle =>
      Path()
        ..addRRect(RRect.fromRectAndRadius(r, Radius.circular(s.width * 0.28))),
    _Mask.rounded =>
      Path()
        ..addRRect(RRect.fromRectAndRadius(r, Radius.circular(s.width * 0.16))),
    _Mask.square => Path()..addRect(r),
  };
}

/// The icon exactly as the system assembles it: a 108 dp layer of which the
/// central 72 are visible, cut by the launcher mask.
class _Adaptive extends StatelessWidget {
  const _Adaptive({
    required this.size,
    this.mask = _Mask.circle,
    this.background = const Color(0xFF12161A),
    this.glyph = Ap.bone100,
    this.guides = false,
  });

  final double size;
  final _Mask mask;
  final Color background;
  final Color glyph;
  final bool guides;

  @override
  Widget build(BuildContext context) => SizedBox.square(
    dimension: size,
    child: CustomPaint(
      painter: _AdaptivePainter(mask, background, glyph, guides),
    ),
  );
}

class _AdaptivePainter extends CustomPainter {
  _AdaptivePainter(this.mask, this.background, this.glyph, this.guides);

  final _Mask mask;
  final Color background;
  final Color glyph;
  final bool guides;

  static final Path _bird = buildPath(
    parsePathData(_pathsOf(foregroundXml()).single),
  );

  @override
  void paint(Canvas canvas, Size size) {
    // The visible window is the central 72 units of the 108 field, and they
    // equal the icon size on screen. So the window's top-left is (18, 18).
    final k = size.width / adaptiveMask;
    const window = (adaptiveViewport - adaptiveMask) / 2;

    canvas.save();
    canvas.clipPath(_maskPath(mask, size));
    canvas.drawRect(Offset.zero & size, Paint()..color = background);
    canvas.translate(-window * k, -window * k);
    canvas.scale(k);
    canvas.drawPath(
      _bird,
      Paint()
        ..color = glyph
        ..isAntiAlias = true,
    );
    canvas.restore();

    if (!guides) return;
    // The 66 dp circle is the zone Android promises to show under any mask.
    final guide = Paint()
      ..style = PaintingStyle.stroke
      ..strokeWidth = 1
      ..color = Ap.ember400.withValues(alpha: 0.75);
    canvas.drawCircle(Offset(size.width / 2, size.height / 2), 33 * k, guide);
  }

  @override
  bool shouldRepaint(_AdaptivePainter old) =>
      old.mask != mask || old.background != background || old.glyph != glyph;
}

/// The Android 7 icon: no mask, we draw the backplate ourselves, over the
/// whole 108 field.
class _Legacy extends StatelessWidget {
  const _Legacy({required this.size});

  final double size;

  @override
  Widget build(BuildContext context) => SizedBox.square(
    dimension: size,
    child: CustomPaint(painter: _LegacyPainter()),
  );
}

class _LegacyPainter extends CustomPainter {
  static final List<Path> _layers = _pathsOf(
    legacyXml(),
  ).map((d) => buildPath(parsePathData(d))).toList();
  static final List<Color> _colors = const [Color(0xFF12161A), Ap.bone100];

  @override
  void paint(Canvas canvas, Size size) {
    final k = size.width / adaptiveViewport;
    canvas.save();
    canvas.scale(k);
    for (var i = 0; i < _layers.length; i++) {
      canvas.drawPath(
        _layers[i],
        Paint()
          ..color = _colors[i]
          ..isAntiAlias = true,
      );
    }
    canvas.restore();
  }

  @override
  bool shouldRepaint(_LegacyPainter old) => false;
}

List<String> _pathsOf(String vectorXml) => RegExp(
  r'android:pathData="([^"]*)"',
).allMatches(vectorXml).map((m) => m.group(1)!).toList();

class _Sheet extends StatelessWidget {
  const _Sheet();

  static const _label = TextStyle(
    fontFamily: 'Inter',
    color: Ap.fog400,
    fontSize: 13,
    letterSpacing: 1.6,
    fontWeight: FontWeight.w600,
  );
  static const _tiny = TextStyle(
    fontFamily: 'Inter',
    color: Ap.stone600,
    fontSize: 11,
  );

  static Widget _cell(Widget child, String caption) => Padding(
    padding: const EdgeInsets.only(right: 24),
    child: Column(
      children: [
        child,
        const SizedBox(height: 8),
        Text(caption, style: _tiny),
      ],
    ),
  );

  @override
  Widget build(BuildContext context) {
    return ColoredBox(
      color: Ap.basalt950,
      child: Padding(
        padding: const EdgeInsets.all(28),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            const Text('LAUNCHER MASKS · THE SAME ICON', style: _label),
            const SizedBox(height: 10),
            Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                for (final (mask, name) in const [
                  (_Mask.circle, 'circle · Pixel'),
                  (_Mask.squircle, 'superellipse · Samsung'),
                  (_Mask.rounded, 'rounded square'),
                  (_Mask.square, 'square'),
                ])
                  _cell(_Adaptive(size: 176, mask: mask), name),
                const SizedBox(width: 20),
                _cell(
                  const _Adaptive(size: 176, guides: true),
                  'in copper — the promised 66 dp',
                ),
              ],
            ),
            const SizedBox(height: 26),

            const Text('WORKING SIZES ON SCREEN', style: _label),
            const SizedBox(height: 10),
            Row(
              crossAxisAlignment: CrossAxisAlignment.end,
              children: [
                for (final px in const [96.0, 64.0, 48.0, 36.0, 28.0])
                  _cell(_Adaptive(size: px), '${px.toInt()} px'),
                const SizedBox(width: 24),
                // On a light home screen the icon sits next to light
                // wallpaper — we check that the dark circle does not merge
                // with the frame.
                Container(
                  color: Ap.bone50,
                  padding: const EdgeInsets.all(16),
                  child: Row(
                    children: const [
                      _Adaptive(size: 64),
                      SizedBox(width: 16),
                      _Adaptive(size: 48),
                    ],
                  ),
                ),
              ],
            ),
            const SizedBox(height: 26),

            const Text('ANDROID 7 · NO MASK, OWN BACKPLATE', style: _label),
            const SizedBox(height: 10),
            Row(
              crossAxisAlignment: CrossAxisAlignment.end,
              children: [
                for (final px in const [176.0, 96.0, 64.0, 48.0])
                  _cell(_Legacy(size: px), '${px.toInt()} px'),
              ],
            ),
            const SizedBox(height: 26),

            const Text(
              'THEMED ICON · ANDROID 13 AND NEWER',
              style: _label,
            ),
            const SizedBox(height: 10),
            Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                _cell(
                  const _Adaptive(
                    size: 132,
                    background: Color(0xFF2A3339),
                    glyph: Color(0xFFBFD4E0),
                  ),
                  'system dark theme',
                ),
                _cell(
                  const _Adaptive(
                    size: 132,
                    background: Color(0xFFDCE4EA),
                    glyph: Color(0xFF32424D),
                  ),
                  'system light theme',
                ),
                const SizedBox(width: 20),
                SizedBox(
                  width: 520,
                  child: Text(
                    'The themed icon colours are set by the system from the '
                    'wallpaper: it recolours our monochrome layer entirely. '
                    'So what to check here is not the colour but whether the '
                    'silhouette reads when contrast drops to the system one.',
                    style: _tiny.copyWith(height: 1.5),
                  ),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}
