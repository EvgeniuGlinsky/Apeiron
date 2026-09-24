/// The text about the vault must match the behaviour.
///
/// The same technique as in `lock_policy_test.dart`, and for the same reason:
/// the screen cannot be checked without a built native library, but the rules
/// by which the text is chosen can. This is also where the most dangerous case
/// is caught: offering to "start over" where the data is in fact intact.
library;

import 'package:apeiron/src/rust/api/vault.dart';
import 'package:apeiron/vault_status.dart';
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

void main() {
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

    test('a transient failure says outright that the data is intact', () {
      final message = describeVault(status(VaultState.retry));
      expect(message.tone, VaultTone.warning);
      expect(message.action, contains('целы'));
      expect(message.action, contains('не потрачена'));
    });
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

    test('setting a PIN warns that a forgotten one loses everything', () {
      final m = describeVault(status(VaultState.pinSetupRequired));
      expect(m.detail, contains('Забытый пин'));
      expect(m.detail, contains('потеря'));
      expect(needsPinSetup(status(VaultState.pinSetupRequired)), isTrue);
      expect(needsPinSetup(status(VaultState.pinMismatch)), isTrue);
    });

    test('a wrong PIN tells the count and the wait', () {
      final m = describeVault(
        status(VaultState.wrongPin, failures: 6, waitSeconds: 30),
      );
      expect(m.detail, contains('6'));
      expect(m.action, contains('30 с'));
    });

    test('waits read as people say them', () {
      expect(waitLabel(30), '30 с');
      expect(waitLabel(60), '1 мин');
      expect(waitLabel(299), '5 мин');
      expect(waitLabel(3600), '1 ч');
    });

    test('a different key is not called a wrong PIN', () {
      final m = describeVault(status(VaultState.keyMismatch));
      expect(m.tone, VaultTone.blocked);
      expect(m.detail, contains('не неверный пин'));
    });
  });

  group('protection level', () {
    test('the raw number is shown next to the name', () {
      final s = status(
        VaultState.opened,
        levelName: 'железо без уточнения',
        levelRaw: -1,
      );
      expect(levelLabel(s), 'железо без уточнения (-1)');
      expect(describeVault(s).detail, contains('(-1)'));
    });

    test('it says "the system reports", not "verified"', () {
      // There is no attestation for symmetric keys: KeyInfo is the framework's
      // self-report within our own process. Calling it verification is wrong.
      for (final backed in [true, false]) {
        final detail = describeVault(
          status(VaultState.opened, hardwareBacked: backed),
        ).detail;
        expect(detail, contains('Система сообщает'));
        expect(detail, isNot(contains('проверено')));
      }
    });

    test('a software key triggers a warning, not silence', () {
      final s = status(
        VaultState.opened,
        levelName: 'программный',
        levelRaw: 0,
        hardwareBacked: false,
      );
      expect(describeVault(s).tone, VaultTone.warning);
      expect(needsHonestWarning(s), isTrue);
      expect(describeVault(s).detail, contains('слабее'));
    });

    test('hardware triggers no warning', () {
      final s = status(VaultState.opened);
      expect(describeVault(s).tone, VaultTone.good);
      expect(needsHonestWarning(s), isFalse);
    });
  });

  group('honesty of wording', () {
    test('success names what the PIN does not survive', () {
      // Forbidden: promising more than is done. Guessing with root on this
      // phone and key extraction from the hardware must be named, not hidden.
      final detail = describeVault(status(VaultState.opened)).detail;
      expect(detail, contains('root'));
      expect(detail, contains('извлекут'));
      expect(detail, contains('длина пина'));
    });

    test('a vanished key explains why recovery is impossible', () {
      final message = describeVault(status(VaultState.keyGone));
      expect(message.tone, VaultTone.blocked);
      expect(message.detail, contains('не выгружался'));
      expect(message.action, contains('не вернётся'));
    });

    test('every state has a title and an explanation', () {
      for (final state in VaultState.values) {
        final message = describeVault(status(state));
        expect(message.title, isNotEmpty, reason: '$state has no title');
        expect(message.detail, isNotEmpty, reason: '$state has no explanation');
      }
    });

    test('a transient failure shows what the platform said', () {
      final message = describeVault(
        status(VaultState.retry, message: 'устройство заперто'),
      );
      expect(message.detail, 'устройство заперто');
    });
  });
}
