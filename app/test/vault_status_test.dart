/// Текст про хранилище обязан совпадать с поведением.
///
/// Тот же приём, что в `lock_policy_test.dart`, и по той же причине: экран без
/// собранной нативной библиотеки не проверить, а вот правила, по которым
/// выбирается текст, — вполне. Здесь же ловится самое опасное: предложение
/// «начать заново» там, где данные на самом деле целы.
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
  group('предложение начать заново', () {
    test('даётся только когда ключ действительно исчез', () {
      expect(mayOfferFreshStart(status(VaultState.keyGone)), isTrue);
    });

    test('не даётся ни в одном другом положении', () {
      for (final state in VaultState.values) {
        if (state == VaultState.keyGone) continue;
        expect(
          mayOfferFreshStart(status(state)),
          isFalse,
          reason:
              'при $state предложено стереть всё, хотя данные целы — это '
              'уничтожение переписки владельца за него',
        );
      }
    });

    test('преходящий отказ прямо говорит, что данные целы', () {
      final message = describeVault(status(VaultState.retry));
      expect(message.tone, VaultTone.warning);
      expect(message.action, contains('целы'));
    });
  });

  group('уровень защиты', () {
    test('сырое число показывается рядом с названием', () {
      final s = status(
        VaultState.opened,
        levelName: 'железо без уточнения',
        levelRaw: -1,
      );
      expect(levelLabel(s), 'железо без уточнения (-1)');
      expect(describeVault(s).detail, contains('(-1)'));
    });

    test('говорится «система сообщает», а не «проверено»', () {
      // Для симметричных ключей аттестации не существует: KeyInfo — самоотчёт
      // фреймворка в нашем же процессе. Называть это проверкой нельзя.
      for (final backed in [true, false]) {
        final detail = describeVault(
          status(VaultState.opened, hardwareBacked: backed),
        ).detail;
        expect(detail, contains('Система сообщает'));
        expect(detail, isNot(contains('проверено')));
      }
    });

    test('программный ключ вызывает предупреждение, а не тишину', () {
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

    test('железо не вызывает предупреждения', () {
      final s = status(VaultState.opened);
      expect(describeVault(s).tone, VaultTone.good);
      expect(needsHonestWarning(s), isFalse);
    });
  });

  group('честность формулировок', () {
    test('успех не обещает того, чего эта задача не даёт', () {
      // Телефон, отобранный разблокированным, закроет пин (R-001), а не
      // аппаратный ключ. Обещать это сейчас — ровно то преувеличение, которое
      // запрещено таблицей формулировок в docs/threat-log.md.
      final detail = describeVault(status(VaultState.opened)).detail;
      expect(detail, contains('разблокированным'));
      expect(detail, contains('пин'));
    });

    test('исчезнувший ключ объясняет, почему восстановить нельзя', () {
      final message = describeVault(status(VaultState.keyGone));
      expect(message.tone, VaultTone.blocked);
      expect(message.detail, contains('не выгружался'));
      expect(message.action, contains('не вернётся'));
    });

    test('у каждого положения есть заголовок и объяснение', () {
      for (final state in VaultState.values) {
        final message = describeVault(status(state));
        expect(message.title, isNotEmpty, reason: 'у $state нет заголовка');
        expect(message.detail, isNotEmpty, reason: 'у $state нет объяснения');
      }
    });

    test('преходящий отказ показывает то, что сказала платформа', () {
      final message = describeVault(
        status(VaultState.retry, message: 'устройство заперто'),
      );
      expect(message.detail, 'устройство заперто');
    });
  });
}
