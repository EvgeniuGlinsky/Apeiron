/// The text about the vault must match the behaviour.
///
/// The same technique as in `lock_policy_test.dart`, and for the same reason:
/// the screen cannot be checked without a built native library, but the rules
/// by which the text is chosen can. This is also where the most dangerous case
/// is caught: offering to "start over" where the data is in fact intact.
///
/// Every promise is checked in every language of the interface: a translation
/// that drops "the data is intact" is as wrong as code that does not keep it.
library;

import 'package:apeiron/l10n/app_localizations.dart';
import 'package:apeiron/src/rust/api/vault.dart';
import 'package:apeiron/vault_status.dart';
import 'package:flutter/widgets.dart' show Locale;
import 'package:flutter_test/flutter_test.dart';

VaultStatus status(
  VaultState state, {
  String message = '',
  String levelName = 'StrongBox',
  int levelRaw = 2,
  bool hardwareBacked = true,
  int failures = 0,
  int waitSeconds = 0,
  int unlockMs = 0,
}) => VaultStatus(
  state: state,
  message: message,
  levelName: levelName,
  levelRaw: levelRaw,
  hardwareBacked: hardwareBacked,
  firstRun: false,
  hasIdentity: true,
  failures: failures,
  waitSeconds: waitSeconds,
  unlockMs: unlockMs,
);

/// The words each promise is made of, per language.
typedef Words = ({
  String intact,
  String notSpent,
  String forgotten,
  String loss,
  String thirtySeconds,
  String notWrongPin,
  String reports,
  String verified,
  String weaker,
  String extracted,
  String pinLength,
  String notExported,
  String notBack,
  String unknownSecure,
  String unknown,
  List<String> waits,
});

const Map<String, Words> words = {
  'ru': (
    intact: 'целы',
    notSpent: 'не потрачена',
    forgotten: 'Забытый пин',
    loss: 'потеря',
    thirtySeconds: '30 с',
    notWrongPin: 'не неверный пин',
    reports: 'Система сообщает',
    verified: 'проверено',
    weaker: 'слабее',
    extracted: 'извлекут',
    pinLength: 'длина пина',
    notExported: 'не выгружался',
    notBack: 'не вернётся',
    unknownSecure: 'железо без уточнения',
    unknown: 'неизвестно',
    waits: ['30 с', '1 мин', '5 мин', '1 ч'],
  ),
  'en': (
    intact: 'intact',
    notSpent: 'not spent',
    forgotten: 'forgotten PIN',
    loss: 'losing all messages',
    thirtySeconds: '30 s',
    notWrongPin: 'not a wrong PIN',
    reports: 'The system reports',
    verified: 'verified',
    weaker: 'weaker',
    extracted: 'extracted',
    pinLength: 'length of the PIN',
    notExported: 'never exported',
    notBack: 'will not come back',
    unknownSecure: 'secure hardware, unspecified',
    unknown: 'unknown',
    waits: ['30 s', '1 min', '5 min', '1 h'],
  ),
};

void main() {
  test('every language of the interface has its words here', () {
    expect(
      AppLocalizations.supportedLocales.map((l) => l.languageCode).toSet(),
      words.keys.toSet(),
      reason: 'a language without its words here would go unchecked',
    );
  });

  group('offer to start over', () {
    const lost = {VaultState.keyGone, VaultState.keyMismatch};

    test('is made only when the key is really lost', () {
      for (final state in lost) {
        expect(mayOfferFreshStart(status(state)), isTrue, reason: '$state');
      }
    });

    test('is not made in any other state', () {
      for (final state in VaultState.values) {
        if (lost.contains(state)) continue;
        expect(
          mayOfferFreshStart(status(state)),
          isFalse,
          reason:
              'in $state erasing everything is offered although the data is '
              "intact — that destroys the owner's messages on their behalf",
        );
      }
    });

    test('a wrong PIN never leads to erasure', () {
      for (final state in [VaultState.wrongPin, VaultState.delayed]) {
        expect(mayOfferFreshStart(status(state, failures: 50)), isFalse);
      }
    });

    test('legacy data has its own, separate way out', () {
      expect(isLegacy(status(VaultState.legacyData)), isTrue);
      expect(mayOfferFreshStart(status(VaultState.legacyData)), isFalse);
    });
  });

  test('the tone does not depend on the language', () {
    for (final state in VaultState.values) {
      for (final backed in [true, false]) {
        final s = status(state, hardwareBacked: backed);
        for (final lang in words.keys) {
          final l = lookupAppLocalizations(Locale(lang));
          expect(describeVault(s, l).tone, vaultTone(s), reason: '$state');
        }
      }
    }
  });

  group('the PIN', () {
    test('the pad is shown exactly when a PIN is expected', () {
      for (final state in VaultState.values) {
        final expected = {
          VaultState.locked,
          VaultState.wrongPin,
          VaultState.delayed,
        }.contains(state);
        expect(needsPin(status(state)), expected, reason: '$state');
      }
    });

    test('setting a PIN is needed exactly when there is none', () {
      expect(needsPinSetup(status(VaultState.pinSetupRequired)), isTrue);
      expect(needsPinSetup(status(VaultState.pinMismatch)), isTrue);
      expect(needsPinSetup(status(VaultState.locked)), isFalse);
    });
  });

  group('protection level', () {
    test('hardware triggers no warning', () {
      final s = status(VaultState.opened);
      expect(vaultTone(s), VaultTone.good);
      expect(needsHonestWarning(s), isFalse);
    });

    test('a software key triggers a warning, not silence', () {
      final s = status(VaultState.opened, levelRaw: 0, hardwareBacked: false);
      expect(vaultTone(s), VaultTone.warning);
      expect(needsHonestWarning(s), isTrue);
    });

    test('the name follows the number, not the text Rust sent', () {
      final l = lookupAppLocalizations(const Locale('en'));
      expect(
        levelLabel(status(VaultState.opened, levelName: 'x', levelRaw: 1), l),
        'TEE (1)',
      );
      expect(
        levelLabel(status(VaultState.opened, levelName: 'x', levelRaw: 2), l),
        'StrongBox (2)',
      );
    });
  });

  for (final MapEntry(key: lang, value: w) in words.entries) {
    final l = lookupAppLocalizations(Locale(lang));

    group('[$lang]', () {
      test('a transient failure says outright that the data is intact', () {
        final message = describeVault(status(VaultState.retry), l);
        expect(message.tone, VaultTone.warning);
        expect(message.action, contains(w.intact));
        expect(message.action, contains(w.notSpent));
      });

      test('setting a PIN warns that a forgotten one loses everything', () {
        final m = describeVault(status(VaultState.pinSetupRequired), l);
        expect(m.detail, contains(w.forgotten));
        expect(m.detail, contains(w.loss));
      });

      test('a wrong PIN tells the count and the wait', () {
        final m = describeVault(
          status(VaultState.wrongPin, failures: 6, waitSeconds: 30),
          l,
        );
        expect(m.detail, contains('6'));
        expect(m.action, contains(w.thirtySeconds));
      });

      test('waits read as people say them', () {
        expect([
          waitLabel(30, l),
          waitLabel(60, l),
          waitLabel(299, l),
          waitLabel(3600, l),
        ], w.waits);
      });

      test('a different key is not called a wrong PIN', () {
        final m = describeVault(status(VaultState.keyMismatch), l);
        expect(m.tone, VaultTone.blocked);
        expect(m.detail, contains(w.notWrongPin));
      });

      test('the raw number is shown next to the name', () {
        final s = status(VaultState.opened, levelRaw: -1);
        expect(levelLabel(s, l), '${w.unknownSecure} (-1)');
        expect(describeVault(s, l).detail, contains('(-1)'));
        expect(
          levelLabel(status(VaultState.opened, levelRaw: 7), l),
          '${w.unknown} (7)',
          reason: 'an unfamiliar level must show as unknown, not as a guess',
        );
      });

      test('it says "the system reports", not "verified"', () {
        // There is no attestation for symmetric keys: KeyInfo is the
        // framework's self-report within our own process. Calling it
        // verification is wrong.
        for (final backed in [true, false]) {
          final detail = describeVault(
            status(VaultState.opened, hardwareBacked: backed),
            l,
          ).detail;
          expect(detail, contains(w.reports));
          expect(detail, isNot(contains(w.verified)));
        }
      });

      test('a software key says the protection is weaker', () {
        final s = status(VaultState.opened, levelRaw: 0, hardwareBacked: false);
        expect(describeVault(s, l).detail, contains(w.weaker));
      });

      test('success names what the PIN does not survive', () {
        // Forbidden: promising more than is done. Guessing with root on this
        // phone and key extraction from the hardware must be named, not hidden.
        final detail = describeVault(status(VaultState.opened), l).detail;
        expect(detail, contains('root'));
        expect(detail, contains(w.extracted));
        expect(detail, contains(w.pinLength));
      });

      test('a vanished key explains why recovery is impossible', () {
        final message = describeVault(status(VaultState.keyGone), l);
        expect(message.tone, VaultTone.blocked);
        expect(message.detail, contains(w.notExported));
        expect(message.action, contains(w.notBack));
      });

      test('every state has a title and an explanation', () {
        for (final state in VaultState.values) {
          final message = describeVault(status(state), l);
          expect(message.title, isNotEmpty, reason: '$state has no title');
          expect(
            message.detail,
            isNotEmpty,
            reason: '$state has no explanation',
          );
        }
      });

      test('a transient failure shows what the platform said', () {
        final message = describeVault(
          status(VaultState.retry, message: 'the device is locked'),
          l,
        );
        expect(message.detail, 'the device is locked');
      });
    });
  }
}
