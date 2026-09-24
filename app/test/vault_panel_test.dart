/// The vault panel renders every state in every language.
///
/// The panel needs no native library — the status is a plain value — so unlike
/// the other screens it can be pumped here. What this catches: a message that
/// throws at runtime, and buttons whose labels do not match the state.
library;

import 'package:apeiron/l10n/app_localizations.dart';
import 'package:apeiron/src/rust/api/vault.dart';
import 'package:apeiron/theme/tokens.dart';
import 'package:apeiron/vault_panel.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

VaultStatus status(VaultState state) => VaultStatus(
  state: state,
  message: '',
  levelName: 'TEE',
  levelRaw: 1,
  hardwareBacked: true,
  firstRun: false,
  hasIdentity: true,
  failures: 6,
  waitSeconds: 300,
  unlockMs: 950,
);

Widget host(Locale locale, Widget child) => MaterialApp(
  theme: Ap.dark(),
  locale: locale,
  localizationsDelegates: AppLocalizations.localizationsDelegates,
  supportedLocales: AppLocalizations.supportedLocales,
  home: Scaffold(body: SingleChildScrollView(child: child)),
);

void main() {
  for (final locale in AppLocalizations.supportedLocales) {
    final l = lookupAppLocalizations(locale);

    testWidgets('[${locale.languageCode}] every state renders', (tester) async {
      for (final state in VaultState.values) {
        await tester.pumpWidget(
          host(
            locale,
            VaultPanel(
              status: status(state),
              onRetry: () {},
              onFreshStart: () {},
              onResetLegacy: () {},
            ),
          ),
        );
        expect(tester.takeException(), isNull, reason: '$state');
      }
    });

    testWidgets(
      '[${locale.languageCode}] "start over" only where the key is lost',
      (tester) async {
        for (final state in VaultState.values) {
          await tester.pumpWidget(
            host(
              locale,
              VaultPanel(status: status(state), onFreshStart: () {}),
            ),
          );
          final offered = find.text(l.startOver).evaluate().isNotEmpty;
          expect(
            offered,
            state == VaultState.keyGone || state == VaultState.keyMismatch,
            reason: '$state',
          );
        }
      },
    );
  }
}
