/// The PIN pad sends positions, never digits (R-004, R-007).
///
/// If the widget ever reported the digit instead of the position, the PIN
/// would be assembled in Dart, where memory cannot be wiped. This is what these
/// tests pin down.
library;

import 'package:apeiron/l10n/app_localizations.dart';
import 'package:apeiron/pin_pad.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

Widget host(Widget child) => MaterialApp(
  localizationsDelegates: AppLocalizations.localizationsDelegates,
  supportedLocales: AppLocalizations.supportedLocales,
  home: Scaffold(body: Center(child: child)),
);

void main() {
  // A layout where no digit sits at its own position, so that a position and a
  // digit can never be confused by accident.
  const layout = [3, 7, 0, 9, 1, 8, 2, 6, 5, 4];

  testWidgets('draws exactly the layout it was given', (tester) async {
    await tester.pumpWidget(
      host(
        PinPad(
          layout: layout,
          entered: 0,
          onPress: (_) {},
          onErase: () {},
          onSubmit: () {},
        ),
      ),
    );
    for (final d in layout) {
      expect(find.text('$d'), findsOneWidget, reason: 'digit $d is not drawn');
    }
  });

  testWidgets('a tap reports the position, not the digit', (tester) async {
    final pressed = <int>[];
    await tester.pumpWidget(
      host(
        PinPad(
          layout: layout,
          entered: 0,
          onPress: pressed.add,
          onErase: () {},
          onSubmit: () {},
        ),
      ),
    );
    // Digit 3 is at position 0, digit 4 at position 9, digit 0 at position 2.
    await tester.tap(find.text('3'));
    await tester.tap(find.text('4'));
    await tester.tap(find.text('0'));
    expect(pressed, [0, 9, 2]);
  });

  testWidgets('submit is possible only from the minimum length', (
    tester,
  ) async {
    var submitted = 0;
    Future<void> pump(int entered) => tester.pumpWidget(
      host(
        PinPad(
          layout: layout,
          entered: entered,
          onPress: (_) {},
          onErase: () {},
          onSubmit: () => submitted++,
        ),
      ),
    );
    await pump(5);
    await tester.tap(find.byIcon(Icons.arrow_forward));
    expect(submitted, 0, reason: 'five digits were submitted');
    await pump(6);
    await tester.tap(find.byIcon(Icons.arrow_forward));
    expect(submitted, 1);
  });

  testWidgets('a busy pad accepts nothing', (tester) async {
    final pressed = <int>[];
    await tester.pumpWidget(
      host(
        PinPad(
          layout: layout,
          entered: 8,
          busy: true,
          onPress: pressed.add,
          onErase: () => pressed.add(-1),
          onSubmit: () => pressed.add(-2),
        ),
      ),
    );
    await tester.tap(find.text('3'));
    await tester.tap(find.byIcon(Icons.backspace_outlined));
    expect(pressed, isEmpty);
  });
}
