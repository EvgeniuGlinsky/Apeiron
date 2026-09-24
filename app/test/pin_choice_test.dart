/// The choice before a PIN is set: 8 digits and the scrambled keyboard unless the owner picks
/// otherwise, and the recommendation is said on screen.
library;

import 'package:apeiron/l10n/app_localizations.dart';
import 'package:apeiron/pin_choice.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

Widget host(Widget child) => MaterialApp(
  locale: const Locale('en'),
  localizationsDelegates: AppLocalizations.localizationsDelegates,
  supportedLocales: AppLocalizations.supportedLocales,
  home: Scaffold(body: SingleChildScrollView(child: child)),
);

void main() {
  testWidgets(
    'recommends 8 digits and the scrambled keyboard, and reports the choice',
    (tester) async {
      final chosen = <(int, bool)>[];
      await tester.pumpWidget(
        host(PinChoice(onChosen: (d, s) => chosen.add((d, s)))),
      );
      final l = await AppLocalizations.delegate.load(const Locale('en'));
      expect(find.text(l.pinChoiceRecommend), findsOneWidget);
      for (final n in pinLengths) {
        expect(find.text(l.pinDigits(n)), findsOneWidget);
      }

      await tester.tap(find.text(l.pinChoiceContinue));
      await tester.tap(find.text(l.pinDigits(4)));
      await tester.tap(find.text(l.pinKeyboardOrdered));
      await tester.pump();
      await tester.tap(find.text(l.pinChoiceContinue));
      expect(chosen, [(8, true), (4, false)]);
    },
  );
}
