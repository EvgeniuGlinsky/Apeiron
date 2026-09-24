import 'package:apeiron/l10n/app_localizations.dart';
import 'package:apeiron/lock_policy.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

/// Test of when the identity locks by itself.
///
/// The logic is short, but a mistake in it is invisible: an unlocked identity
/// looks exactly like one locked on time. So it is separated from the widget
/// and tested here in full.
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

  group('phone', () {
    test('locks on focus loss — decision R-001', () {
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

    test('no idle timer: the screen turning off plays its role', () {
      for (final p in mobile) {
        expect(LockPolicy.of(p).idleTimeout, isNull, reason: '$p');
      }
    });
  });

  group('desktop', () {
    test('switching to another window leaves the identity alone', () {
      for (final p in desktop) {
        expect(
          LockPolicy.of(p).locksOn(AppLifecycleState.inactive),
          isFalse,
          reason: '$p',
        );
      }
    });

    test('a minimised window and the app leaving both lock', () {
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

    test('an idle timer instead of focus', () {
      for (final p in desktop) {
        final timeout = LockPolicy.of(p).idleTimeout;
        expect(timeout, isNotNull, reason: '$p');
        // Not "just any" time: minutes, not hours and not seconds.
        expect(timeout!.inMinutes, inInclusiveRange(1, 15), reason: '$p');
      }
    });
  });

  test('resuming does not lock on any platform', () {
    for (final p in [...mobile, ...desktop]) {
      expect(
        LockPolicy.of(p).locksOn(AppLifecycleState.resumed),
        isFalse,
        reason: '$p',
      );
    }
  });

  // The same promise in every language: the words differ, the behaviour named
  // must not.
  const words = {
    'ru': (background: 'в фон', minimised: 'свёрнут', minutes: 'минут'),
    'en': (background: 'background', minimised: 'minimised', minutes: 'minute'),
  };

  test('every language of the interface is checked below', () {
    expect(
      AppLocalizations.supportedLocales.map((l) => l.languageCode).toSet(),
      words.keys.toSet(),
    );
  });

  for (final MapEntry(key: lang, value: w) in words.entries) {
    final l = lookupAppLocalizations(Locale(lang));

    group('[$lang] explanation matches behaviour', () {
      test('on the phone, background is promised', () {
        expect(
          LockPolicy.of(TargetPlatform.android).explanation(l),
          contains(w.background),
        );
      });

      test('desktop: window and timeout promised, timeout named honestly', () {
        final policy = LockPolicy.of(TargetPlatform.windows);
        final text = policy.explanation(l);
        expect(text, contains(w.minimised));
        expect(
          text,
          contains('${policy.idleTimeout!.inMinutes} ${w.minutes}'),
          reason: 'the text must state the same timeout as the code',
        );
        expect(text, isNot(contains(w.background)));
      });

      test('the timeout in the text follows the code, not the translation', () {
        const policy = LockPolicy(
          locksOnFocusLoss: false,
          idleTimeout: Duration(minutes: 7),
        );
        expect(policy.explanation(l), contains('7 ${w.minutes}'));
      });
    });
  }
}
