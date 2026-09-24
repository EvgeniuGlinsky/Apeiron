import 'dart:io';
import 'dart:ui' as ui;

import 'package:apeiron/theme/tokens.dart';
import 'package:apeiron/wordmark.dart';
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart' show FontLoader, rootBundle;
import 'package:flutter_test/flutter_test.dart';

/// A tool, not a test: lays out the APEIRON wordmark at various sizes and on
/// various backgrounds.
///
///     flutter test test/wordmark_sheet.dart
///
/// Output — `build/mark/wordmark-sheet.png`.
///
/// The name lacks the `_test` suffix on purpose — see `mark_sheet.dart`.
void main() {
  testWidgets('wordmark sheet', (tester) async {
    await (FontLoader(
      'Inter',
    )..addFont(rootBundle.load('assets/fonts/Inter-SemiBold.otf'))).load();

    const sheet = Size(1500, 980);
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
    final out = File('${dir.path}/wordmark-sheet.png')
      ..writeAsBytesSync(data!.buffer.asUint8List());

    expect(out.lengthSync(), greaterThan(0));
    // ignore: avoid_print
    print('Wordmark sheet: ${out.absolute.path}');
  });
}

class _Sheet extends StatelessWidget {
  const _Sheet();

  static const _label = TextStyle(
    fontFamily: 'Inter',
    color: Ap.fog400,
    fontSize: 13,
    letterSpacing: 1.8,
    fontWeight: FontWeight.w600,
  );

  @override
  Widget build(BuildContext context) {
    return ColoredBox(
      color: Ap.basalt950,
      child: Padding(
        padding: const EdgeInsets.all(34),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            const Text('LARGE — LETTER SHAPES', style: _label),
            const SizedBox(height: 16),
            const ApeironWordmark(height: 96),
            const SizedBox(height: 34),

            const Text('WORKING SIZES', style: _label),
            const SizedBox(height: 16),
            for (final h in [44.0, 28.0, 20.0, 14.0]) ...[
              Row(
                crossAxisAlignment: CrossAxisAlignment.center,
                children: [
                  SizedBox(width: 420, child: ApeironWordmark(height: h)),
                  Text('${h.toInt()} px', style: _label),
                ],
              ),
              const SizedBox(height: 14),
            ],
            const SizedBox(height: 16),

            const Text('BACKGROUNDS AND STROKE WEIGHT', style: _label),
            const SizedBox(height: 16),
            Row(
              children: [
                // Light: print and the light theme — half of all uses.
                Container(
                  width: 360,
                  height: 110,
                  color: Ap.bone50,
                  alignment: Alignment.center,
                  child: const ApeironWordmark(height: 34, color: Ap.basalt900),
                ),
                const SizedBox(width: 22),
                // Accent — as in the app bar.
                Container(
                  width: 360,
                  height: 110,
                  color: Ap.basalt800,
                  alignment: Alignment.center,
                  child: const ApeironWordmark(
                    height: 34,
                    color: Ap.glacier400,
                  ),
                ),
                const SizedBox(width: 22),
                // Thinner and thicker: checking at what weight the counters
                // of P, R and O fill in.
                Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: const [
                    ApeironWordmark(height: 34, strokeRatio: 1 / 13),
                    SizedBox(height: 14),
                    ApeironWordmark(height: 34, strokeRatio: 1 / 9),
                    SizedBox(height: 14),
                    ApeironWordmark(height: 34, strokeRatio: 1 / 6),
                  ],
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}
