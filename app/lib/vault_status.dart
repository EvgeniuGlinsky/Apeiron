/// What to tell a person about the key vault.
///
/// The logic is split out of the widget following [lock_policy.dart] and for
/// the same reason: it is checked by tests, while the screen cannot be checked
/// at all without a built native library. The rule from there carries over
/// literally — **the text must match the behaviour**.
///
/// The wording here is not decoration. The table in `docs/threat-log.md`
/// explicitly forbids promising more than has been done. With the PIN (R-011)
/// a phone taken away unlocked no longer opens the vault — but a guess is still
/// possible on this very phone for someone with root, and extraction of the key
/// from the hardware leaves only the length of the PIN. That is what is said,
/// in every language: the texts live in `lib/l10n/*.arb`, and the tests check
/// the same promises in each of them.
library;

import 'l10n/app_localizations.dart';
import 'src/rust/api/vault.dart' show VaultState, VaultStatus;

/// How serious the state is.
enum VaultTone {
  /// Everything as designed.
  good,

  /// Works, but weaker than promised, or needs a step. Staying silent is not
  /// allowed.
  warning,

  /// Cannot go further without the person's decision.
  blocked,
}

/// Ready-made text for the screen.
class VaultMessage {
  const VaultMessage({
    required this.tone,
    required this.title,
    required this.detail,
    required this.action,
  });

  /// Tone: green, warning or dead end.
  final VaultTone tone;

  /// A one-line heading.
  final String title;

  /// Explanation. Neither softens nor over-promises.
  final String detail;

  /// What the person can do. Empty string if there is nothing to do.
  final String action;
}

/// Raw security levels as `KeyProperties` defines them (and `platform/src/lib.rs`).
const _levelSoftware = 0;
const _levelTee = 1;
const _levelStrongBox = 2;
const _levelUnknownSecure = -1;

/// Level name together with the raw number.
///
/// The raw number is shown alongside on purpose: the system has five values,
/// not three, and an unfamiliar one must be visible rather than replaced by
/// the nearest familiar one. The name is chosen here by the number, in the
/// interface language; the name Rust sends stays for the diagnostics.
String levelLabel(VaultStatus status, AppLocalizations l) {
  final name = switch (status.levelRaw) {
    _levelStrongBox => 'StrongBox',
    _levelTee => 'TEE',
    _levelSoftware => l.levelSoftware,
    _levelUnknownSecure => l.levelUnknownSecure,
    _ => l.levelUnknown,
  };
  return '$name (${status.levelRaw})';
}

/// A wait as people say it: "40 s", "5 min", "1 h".
String waitLabel(int seconds, AppLocalizations l) {
  if (seconds < 60) return l.waitSeconds(seconds);
  final minutes = (seconds / 60).ceil();
  if (minutes < 60) return l.waitMinutes(minutes);
  return l.waitHours((minutes / 60).ceil());
}

/// How serious the current state of the vault is. Does not depend on the
/// language, so the decisions made on it do not either.
VaultTone vaultTone(VaultStatus status) => switch (status.state) {
  VaultState.keyGone ||
  VaultState.keyMismatch ||
  VaultState.legacyData ||
  VaultState.unavailable => VaultTone.blocked,
  VaultState.pinSetupRequired ||
  VaultState.pinMismatch ||
  VaultState.wrongPin ||
  VaultState.delayed ||
  VaultState.retry ||
  VaultState.locked => VaultTone.warning,
  // R-002 explicitly requires: where there is no hardware keystore — an
  // honest warning, not a silent fallback to a weak scheme.
  VaultState.opened =>
    status.hardwareBacked ? VaultTone.good : VaultTone.warning,
};

/// What to show for the current state of the vault.
VaultMessage describeVault(VaultStatus status, AppLocalizations l) {
  VaultMessage say(String title, String detail, [String action = '']) =>
      VaultMessage(
        tone: vaultTone(status),
        title: title,
        detail: detail,
        action: action,
      );

  switch (status.state) {
    case VaultState.keyGone:
      return say(
        l.vaultKeyGoneTitle,
        '${l.vaultKeyGoneDetail} ${l.keyNotExported}',
        l.startOverAction,
      );

    case VaultState.keyMismatch:
      return say(
        l.vaultKeyMismatchTitle,
        '${l.vaultKeyMismatchDetail} ${l.keyNotExported}',
        l.startOverAction,
      );

    case VaultState.legacyData:
      return say(l.vaultLegacyTitle, l.vaultLegacyDetail, l.vaultLegacyAction);

    case VaultState.pinSetupRequired:
      return say(
        l.vaultPinSetupTitle,
        l.vaultPinSetupDetail,
        l.vaultPinSetupAction,
      );

    case VaultState.pinMismatch:
      return say(
        l.vaultPinMismatchTitle,
        l.vaultPinMismatchDetail,
        l.vaultPinMismatchAction,
      );

    case VaultState.wrongPin:
      return say(
        l.vaultWrongPinTitle,
        l.vaultWrongPinDetail(status.failures),
        status.waitSeconds > 0
            ? l.nextAttemptIn(waitLabel(status.waitSeconds, l))
            : l.tryAgain,
      );

    case VaultState.delayed:
      return say(
        l.vaultDelayedTitle,
        l.vaultDelayedDetail(status.failures),
        l.nextAttemptIn(waitLabel(status.waitSeconds, l)),
      );

    case VaultState.retry:
      // The platform's own words, when there are any: they are diagnostics,
      // and replacing them with a friendlier guess would hide the cause.
      return say(
        l.vaultRetryTitle,
        status.message.isEmpty ? l.vaultRetryFallback : status.message,
        l.vaultRetryAction,
      );

    case VaultState.unavailable:
      return say(l.vaultUnavailableTitle, l.vaultUnavailableDetail);

    case VaultState.locked:
      return say(l.vaultLockedTitle, l.vaultLockedDetail, l.vaultLockedAction);

    case VaultState.opened:
      if (!status.hardwareBacked) {
        return say(
          l.vaultSoftwareTitle,
          l.vaultSoftwareDetail(levelLabel(status, l)),
        );
      }
      return say(
        l.vaultOpenedTitle,
        l.vaultOpenedDetail(levelLabel(status, l)),
        status.unlockMs > 0 ? l.vaultUnlockTook(status.unlockMs) : '',
      );
  }
}

/// Whether "start over" may be offered.
///
/// Only when the key is really lost. Offering to erase everything on a
/// transient firmware failure or a wrong PIN means destroying the owner's
/// messages on their behalf.
bool mayOfferFreshStart(VaultStatus status) =>
    status.state == VaultState.keyGone ||
    status.state == VaultState.keyMismatch;

/// Whether this is the state of a test build's data from before the PIN.
bool isLegacy(VaultStatus status) => status.state == VaultState.legacyData;

/// Whether the PIN pad is what the person should see now.
bool needsPin(VaultStatus status) => const {
  VaultState.locked,
  VaultState.wrongPin,
  VaultState.delayed,
}.contains(status.state);

/// Whether a new PIN has to be set.
bool needsPinSetup(VaultStatus status) =>
    status.state == VaultState.pinSetupRequired ||
    status.state == VaultState.pinMismatch;

/// Whether to show a warning next to the normal screen.
bool needsHonestWarning(VaultStatus status) =>
    vaultTone(status) != VaultTone.good;
