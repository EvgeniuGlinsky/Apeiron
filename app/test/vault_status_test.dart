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
}) => VaultStatus(
  state: state,
  message: message,
  levelName: levelName,
  levelRaw: levelRaw,
  hardwareBacked: hardwareBacked,
  firstRun: false,
  hasIdentity: true,
);

void main() {
  group('offer to start over', () {
    test('is made only when the key is really gone', () {
      expect(mayOfferFreshStart(status(VaultState.keyGone)), isTrue);
    });

    test('is not made in any other state', () {
      for (final state in VaultState.values) {
        if (state == VaultState.keyGone) continue;
        expect(
          mayOfferFreshStart(status(state)),
          isFalse,
          reason:
              'in $state erasing everything is offered although the data is '
              "intact — that destroys the owner's messages on their behalf",
        );
      }
    });

    test('a transient failure says outright that the data is intact', () {
      final message = describeVault(status(VaultState.retry));
      expect(message.tone, VaultTone.warning);
      expect(message.action, contains('целы'));
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
    test('success does not promise what this task does not deliver', () {
      // A phone taken away while unlocked will be covered by the PIN (R-001),
      // not the hardware key. Promising it now is exactly the overstatement
      // forbidden by the wording table in docs/threat-log.md.
      final detail = describeVault(status(VaultState.opened)).detail;
      expect(detail, contains('разблокированным'));
      expect(detail, contains('пин'));
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
