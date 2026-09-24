import 'package:apeiron/fingerprint.dart';
import 'package:apeiron/svg_path.dart';
import 'package:apeiron/raven.dart';
import 'package:apeiron/theme/tokens.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

/// Tests of the design system.
///
/// Screens are not tested here: `RustLib.init()` requires a built native
/// library, which makes such a test an integration test. These checks guard
/// the rules from `docs/design.md` that are easy to break without noticing.
void main() {
  group('square corner rule', () {
    test('cards are not rounded', () {
      final shape = Ap.dark().cardTheme.shape;
      expect(shape, isA<RoundedRectangleBorder>());
      expect(
        (shape! as RoundedRectangleBorder).borderRadius,
        BorderRadius.zero,
        reason: "rounding breaks the system's only hard rule",
      );
    });

    test('buttons are not rounded', () {
      final style = Ap.dark().filledButtonTheme.style;
      final shape = style?.shape?.resolve(<WidgetState>{});
      expect(shape, isA<RoundedRectangleBorder>());
      expect(
        (shape! as RoundedRectangleBorder).borderRadius,
        BorderRadius.zero,
      );
    });
  });

  group('palette', () {
    test('body text is not pure white', () {
      // Pure white on dark strains the eye and looks cheap.
      expect(Ap.bone100, isNot(const Color(0xFFFFFFFF)));
    });

    test('copper is reserved and distinct from glacier', () {
      expect(Ap.ember400, isNot(Ap.glacier400));
    });
  });

  group('monospace font', () {
    test('JetBrains Mono comes first, fallbacks are set', () {
      // Legibility of 0/O and 1/l in a safety number is a security matter,
      // not taste: confusion means a missed man in the middle.
      expect(Ap.monoFallback.first, 'JetBrains Mono');
      expect(Ap.monoFallback.length, greaterThan(2));
      expect(Ap.monoFallback.last, 'monospace');
    });

    test('the mono() style applies the typeface and fallbacks', () {
      final s = Ap.mono(size: 26, spacing: 3.4);
      expect(s.fontFamily, 'JetBrains Mono');
      expect(s.fontFamilyFallback, isNotEmpty);
      expect(s.fontSize, 26);
      expect(s.letterSpacing, 3.4);
    });
  });

  group('fingerprint layout', () {
    test('six groups fit into two rows of three', () {
      final rows = fingerprintRows([
        '30879',
        '28053',
        '14932',
        '68733',
        '97238',
        '94718',
      ], 3);
      expect(rows.length, 2);
      expect(rows[0], ['30879', '28053', '14932']);
      expect(rows[1], ['68733', '97238', '94718']);
    });

    test('an incomplete last row neither crashes nor loses groups', () {
      final rows = fingerprintRows(['a', 'b', 'c', 'd'], 3);
      expect(rows, [
        ['a', 'b', 'c'],
        ['d'],
      ]);
      expect(rows.expand((r) => r).length, 4, reason: 'no group is lost');
    });

    test('empty input gives an empty result, not an exception', () {
      expect(fingerprintRows(const [], 3), isEmpty);
    });

    test('per <= 0 does not loop forever', () {
      // Guard against an infinite loop: a zero step would hang the
      // verification screen.
      expect(fingerprintRows(['a', 'b'], 0), [
        ['a', 'b'],
      ]);
      expect(fingerprintRows(['a', 'b'], -3), [
        ['a', 'b'],
      ]);
    });

    test('groups keep their order under any split', () {
      final src = List.generate(7, (i) => '$i');
      for (final per in [1, 2, 3, 5, 7, 20]) {
        expect(
          fingerprintRows(src, per).expand((r) => r).toList(),
          src,
          reason: 'order broken at per=$per',
        );
      }
    });
  });

  group('SVG path parsing', () {
    test('a simple outline parses and gives the expected bounds', () {
      final p = parseSvgPath('M 10 20 L 40 20 L 40 60 Z');
      final b = p.getBounds();
      expect(b.left, 10);
      expect(b.top, 20);
      expect(b.right, 40);
      expect(b.bottom, 60);
    });

    test('relative commands are measured from the current point', () {
      final abs = parseSvgPath('M 0 0 L 10 0 L 10 10 Z');
      final rel = parseSvgPath('m 0 0 l 10 0 l 0 10 z');
      expect(rel.getBounds(), abs.getBounds());
    });

    test('repeated coordinates after M mean lines', () {
      // The second pair after M is a lineTo, not another moveTo.
      final p = parseSvgPath('M 0 0 10 10');
      expect(p.getBounds().right, 10);
    });

    test('an unsupported command is not drawn silently but throws', () {
      // Silently ignoring it would give a wrong outline with no sign at all.
      expect(
        () => parseSvgPath('M 0 0 A 5 5 0 0 1 10 10'),
        throwsA(isA<FormatException>()),
      );
    });

    test('a truncated path throws', () {
      expect(() => parseSvgPath('M 0 0 L 10'), throwsA(isA<FormatException>()));
    });
  });

  group('fitting and transforms', () {
    test('fitPath fits into the field, preserving proportions', () {
      final p = parseSvgPath('M 0 0 L 100 0 L 100 50 Z');
      final f = fitPath(p, const Rect.fromLTWH(0, 0, 200, 200));
      final b = f.getBounds();
      expect(b.width, closeTo(200, 0.01));
      expect(b.height, closeTo(100, 0.01));
      // Centred vertically.
      expect(b.top, closeTo(50, 0.01));
    });

    test('mirroring does not change the dimensions', () {
      final p = parseSvgPath('M 0 0 L 100 0 L 100 50 Z');
      expect(mirrorPathX(p).getBounds(), p.getBounds());
    });

    test('rotating by 90° swaps width and height', () {
      final p = parseSvgPath('M 0 0 L 100 0 L 100 50 L 0 50 Z');
      final r = rotatePath(p, 90).getBounds();
      expect(r.width, closeTo(50, 0.01));
      expect(r.height, closeTo(100, 0.01));
    });

    test('rotating by zero returns the same path', () {
      final p = parseSvgPath('M 0 0 L 10 10 Z');
      expect(identical(rotatePath(p, 0), p), isTrue);
    });
  });

  testWidgets('the raven renders', (tester) async {
    await tester.pumpWidget(
      const MaterialApp(
        home: Scaffold(body: Center(child: ApeironRaven(size: 64))),
      ),
    );
    expect(find.byType(ApeironRaven), findsOneWidget);
    expect(tester.takeException(), isNull);
  });
}
