import 'dart:io';
import 'dart:ui' as ui;

import 'package:apeiron/app_icon.dart';
import 'package:apeiron/raven.dart';
import 'package:apeiron/theme/tokens.dart';
import 'package:apeiron/wordmark.dart';
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart' show FontLoader, rootBundle;
import 'package:flutter_test/flutter_test.dart';

/// A tool, not a test: lays out the raven and the app icon.
///
///     flutter test test/raven_sheet.dart
///
/// Output — `build/mark/raven-sheet.png`.
///
/// The name lacks the `_test` suffix on purpose — see `wordmark_sheet.dart`.
void main() {
  testWidgets('raven sheet', (tester) async {
    await (FontLoader(
      'Inter',
    )..addFont(rootBundle.load('assets/fonts/Inter-SemiBold.otf'))).load();

    const sheet = Size(1580, 1420);
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
    final out = File('${dir.path}/raven-sheet.png')
      ..writeAsBytesSync(data!.buffer.asUint8List());

    expect(out.lengthSync(), greaterThan(0));
    // ignore: avoid_print
    print('Raven sheet: ${out.absolute.path}');
  });
}

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

  @override
  Widget build(BuildContext context) {
    return ColoredBox(
      color: Ap.basalt950,
      child: Padding(
        padding: const EdgeInsets.all(28),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            // The silhouette on its own, at various sizes.
            for (final pitch in [0.0, -20.0, -32.0, -45.0]) ...[
              Text('PITCH ${pitch.toInt()}°', style: _label),
              const SizedBox(height: 8),
              Row(
                children: [
                  for (final px in [128.0, 64.0, 32.0, 24.0, 16.0])
                    Padding(
                      padding: const EdgeInsets.only(right: 24),
                      child: Column(
                        children: [
                          SizedBox(
                            width: 136,
                            height: 136,
                            child: Center(
                              child: ApeironRaven(
                                size: px,
                                color: Ap.bone100,
                                pitchDegrees: pitch,
                              ),
                            ),
                          ),
                          Text('${px.toInt()} px', style: _tiny),
                        ],
                      ),
                    ),
                  Container(
                    width: 136,
                    height: 136,
                    color: Ap.bone50,
                    alignment: Alignment.center,
                    child: ApeironRaven(
                      size: 78,
                      color: Ap.basalt900,
                      pitchDegrees: pitch,
                    ),
                  ),
                  const SizedBox(width: 20),
                  Container(
                    width: 300,
                    height: 136,
                    color: Ap.basalt900,
                    alignment: Alignment.center,
                    child: Row(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        ApeironRaven(
                          size: 30,
                          color: Ap.bone100,
                          pitchDegrees: pitch,
                        ),
                        const SizedBox(width: 14),
                        const ApeironWordmark(height: 24),
                      ],
                    ),
                  ),
                ],
              ),
              const SizedBox(height: 16),
            ],

            const Text('APP ICON · BACKPLATE AND COLOUR', style: _label),
            const SizedBox(height: 10),
            Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                for (final pitch in const [0.0, -20.0, -32.0, -45.0])
                  Padding(
                    padding: const EdgeInsets.only(right: 26),
                    child: Column(
                      children: [
                        ApeironAppIcon(size: 132, pitchDegrees: pitch),
                        const SizedBox(height: 8),
                        Text('${pitch.toInt()}°', style: _tiny),
                      ],
                    ),
                  ),
                // Chamfered square versus circle.
                Column(
                  children: [
                    const ApeironAppIcon(
                      size: 132,
                      background: Ap.basalt900,
                      shell: IconShell.chamfered,
                    ),
                    const SizedBox(height: 8),
                    const Text('chamfered square', style: _tiny),
                  ],
                ),
                const SizedBox(width: 26),
                // Working icon sizes on a phone screen.
                Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Row(
                      crossAxisAlignment: CrossAxisAlignment.end,
                      children: [
                        for (final px in [64.0, 48.0, 32.0, 24.0])
                          Padding(
                            padding: const EdgeInsets.only(right: 16),
                            child: Column(
                              children: [
                                ApeironAppIcon(size: px),
                                const SizedBox(height: 6),
                                Text('${px.toInt()}', style: _tiny),
                              ],
                            ),
                          ),
                      ],
                    ),
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
