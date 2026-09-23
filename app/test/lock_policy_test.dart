import 'package:apeiron/lock_policy.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

/// Проверка того, когда личность запирается сама.
///
/// Логика короткая, но ошибка в ней не видна: незапертая личность выглядит
/// точно так же, как запертая вовремя. Поэтому она отделена от виджета
/// и проверяется здесь целиком.
void main() {
  const mobile = [
    TargetPlatform.android,
    TargetPlatform.iOS,
    TargetPlatform.fuchsia,
  ];
  const desktop = [
    TargetPlatform.windows,
    TargetPlatform.macOS,
    TargetPlatform.linux,
  ];

  group('телефон', () {
    test('запирается при потере фокуса — решение R-001', () {
      for (final p in mobile) {
        final policy = LockPolicy.of(p);
        expect(
          policy.locksOn(AppLifecycleState.inactive),
          isTrue,
          reason: '$p',
        );
        expect(policy.locksOn(AppLifecycleState.paused), isTrue, reason: '$p');
        expect(policy.locksOn(AppLifecycleState.hidden), isTrue, reason: '$p');
        expect(
          policy.locksOn(AppLifecycleState.detached),
          isTrue,
          reason: '$p',
        );
      }
    });

    test('таймера бездействия нет: его роль играет гашение экрана', () {
      for (final p in mobile) {
        expect(LockPolicy.of(p).idleTimeout, isNull, reason: '$p');
      }
    });
  });

  group('рабочий стол', () {
    test('переключение на другое окно личность не трогает', () {
      for (final p in desktop) {
        expect(
          LockPolicy.of(p).locksOn(AppLifecycleState.inactive),
          isFalse,
          reason: '$p',
        );
      }
    });

    test('свёрнутое окно и уход приложения запирают', () {
      for (final p in desktop) {
        final policy = LockPolicy.of(p);
        expect(policy.locksOn(AppLifecycleState.hidden), isTrue, reason: '$p');
        expect(policy.locksOn(AppLifecycleState.paused), isTrue, reason: '$p');
        expect(
          policy.locksOn(AppLifecycleState.detached),
          isTrue,
          reason: '$p',
        );
      }
    });

    test('вместо фокуса — таймер бездействия', () {
      for (final p in desktop) {
        final timeout = LockPolicy.of(p).idleTimeout;
        expect(timeout, isNotNull, reason: '$p');
        // Не «какое-нибудь» время: минуты, а не часы и не секунды.
        expect(timeout!.inMinutes, inInclusiveRange(1, 15), reason: '$p');
      }
    });
  });

  test('возврат в работу не запирает ни на одной платформе', () {
    for (final p in [...mobile, ...desktop]) {
      expect(
        LockPolicy.of(p).locksOn(AppLifecycleState.resumed),
        isFalse,
        reason: '$p',
      );
    }
  });

  group('объяснение совпадает с поведением', () {
    test('на телефоне обещан фон', () {
      expect(
        LockPolicy.of(TargetPlatform.android).explanation,
        contains('фон'),
      );
    });

    test('на рабочем столе обещаны окно и срок, и он назван честно', () {
      final policy = LockPolicy.of(TargetPlatform.windows);
      expect(policy.explanation, contains('свёрнут'));
      expect(
        policy.explanation,
        contains('${policy.idleTimeout!.inMinutes} минут'),
        reason: 'в тексте должен стоять тот же срок, что и в коде',
      );
      expect(policy.explanation, isNot(contains('в фон')));
    });
  });
}
