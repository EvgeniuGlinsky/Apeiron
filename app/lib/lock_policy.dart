import 'package:flutter/widgets.dart';

import 'l10n/app_localizations.dart';

/// When the identity locks by itself.
///
/// Decision R-001 ("PIN on every return") was written for the phone: the most
/// common coercion scenario is a device taken away while unlocked. On a phone
/// the app leaving the foreground is exactly the moment protection must kick
/// in, and it also coincides with the screen turning off: the system itself
/// moves the app to the background, so no separate idle timer is needed there.
///
/// **On the desktop this rule does harm.** There, losing focus is a click into
/// the browser, not the device leaving your hands; locking on every window
/// switch turns protection into a nuisance, and nuisances get switched off. So
/// on the desktop focus is not treated as an event, but something the phone
/// lacks appears: an **idle timer**. The equivalent of "the phone was taken"
/// here is "stepped away from the computer", and that is exactly how it is
/// measured.
///
/// The logic is split out of the widget so it can be tested without running
/// the app: the cost of a mistake here is a silently unlocked identity.
@immutable
class LockPolicy {
  const LockPolicy({required this.locksOnFocusLoss, required this.idleTimeout});

  /// Policy for the platform the app is running on.
  factory LockPolicy.of(TargetPlatform platform) => switch (platform) {
    TargetPlatform.android || TargetPlatform.iOS || TargetPlatform.fuchsia =>
      const LockPolicy(locksOnFocusLoss: true, idleTimeout: null),
    TargetPlatform.windows ||
    TargetPlatform.macOS ||
    TargetPlatform.linux => const LockPolicy(
      locksOnFocusLoss: false,
      // Three minutes: less and a person cannot finish reading a long message
      // and get back to the keyboard; more and anyone at all has time to walk
      // up to the open screen.
      idleTimeout: Duration(minutes: 3),
    ),
  };

  /// Whether to lock on `inactive` — losing focus without leaving the screen.
  final bool locksOnFocusLoss;

  /// After how much inactivity to lock. `null` — not time-based.
  final Duration? idleTimeout;

  /// Whether to lock on transition to [state].
  bool locksOn(AppLifecycleState state) => switch (state) {
    AppLifecycleState.resumed => false,
    // The app is still on screen, but input goes to another window.
    AppLifecycleState.inactive => locksOnFocusLoss,
    // Minimised, covered, or the app is leaving for good — lock in every case.
    AppLifecycleState.hidden ||
    AppLifecycleState.paused ||
    AppLifecycleState.detached => true,
  };

  /// How to explain this to the user. The promise in the UI must match what
  /// the code does on **this** platform, not in general — and the timeout in
  /// the text is the one in the code, not a number written into a translation.
  String explanation(AppLocalizations l) {
    if (locksOnFocusLoss) return l.lockExplainMobile;
    final minutes = idleTimeout?.inMinutes;
    return minutes == null
        ? l.lockExplainDesktopNoTimeout
        : l.lockExplainDesktop(minutes);
  }

  @override
  bool operator ==(Object other) =>
      other is LockPolicy &&
      other.locksOnFocusLoss == locksOnFocusLoss &&
      other.idleTimeout == idleTimeout;

  @override
  int get hashCode => Object.hash(locksOnFocusLoss, idleTimeout);
}
