// ignore: unused_import
import 'package:intl/intl.dart' as intl;
import 'app_localizations.dart';

// ignore_for_file: type=lint

/// The translations for English (`en`).
class AppLocalizationsEn extends AppLocalizations {
  AppLocalizationsEn([String locale = 'en']) : super(locale);

  @override
  String get selfCheckTooltip => 'Self-check';

  @override
  String get lockTooltip => 'Lock';

  @override
  String get noIdentityTitle => 'NO IDENTITY YET';

  @override
  String get noIdentityBody =>
      'The vault on this device has no identity yet. The device keys and the identity log are created together with it — apart they are meaningless.';

  @override
  String get createIdentity => 'CREATE IDENTITY';

  @override
  String get fingerprintTitle => 'FINGERPRINT';

  @override
  String get fingerprintBody =>
      'This is read aloud to the person you talk to. A difference in even one digit means someone is between you.';

  @override
  String get notVerified => 'NOT VERIFIED WITH ANYONE';

  @override
  String get keySigning => 'ED25519 · SIGNING';

  @override
  String get keyAgreement => 'X25519 · AGREEMENT';

  @override
  String get copy => 'Copy';

  @override
  String get copied => 'Copied';

  @override
  String get trueNowTitle => 'WHAT IS ALREADY TRUE HERE';

  @override
  String trueNowBody(String explanation) {
    return 'Secret keys never leave Rust — only the public halves and the fingerprint came here. $explanation';
  }

  @override
  String get notYetTitle => 'WHAT IS NOT HERE YET';

  @override
  String get notYetBody =>
      'There are no conversations yet. The app goes online only for the DHT measurement on the self-check screen: test envelopes with no content.';

  @override
  String get lockExplainMobile =>
      'When the app goes to the background, the identity is destroyed together with the keys.';

  @override
  String lockExplainDesktop(int minutes) {
    String _temp0 = intl.Intl.pluralLogic(
      minutes,
      locale: localeName,
      other: '$minutes minutes',
      one: '1 minute',
    );
    return 'The identity is destroyed together with the keys when the window is minimised or there is no activity for $_temp0. Switching to another window leaves it alone: on a desktop that happens too often to mean anything.';
  }

  @override
  String get lockExplainDesktopNoTimeout =>
      'The identity is destroyed together with the keys when the window is minimised. Switching to another window leaves it alone: on a desktop that happens too often to mean anything.';

  @override
  String get keyNotExported =>
      'The key lived in this phone\'s secure hardware and was never exported from it — that was the point — so there is nowhere to get it back from.';

  @override
  String get startOverAction =>
      'Start over. Earlier messages will not come back.';

  @override
  String get vaultKeyGoneTitle => 'This phone\'s key is gone';

  @override
  String get vaultKeyGoneDetail =>
      'The vault key has disappeared from the secure hardware. This happens after a firmware update, when the screen lock is removed, and when data is restored from a backup.';

  @override
  String get vaultKeyMismatchTitle =>
      'The key in the secure hardware is the wrong one';

  @override
  String get vaultKeyMismatchDetail =>
      'There is a key in the secure hardware, but not the one the vault was made with. This is not a wrong PIN: no attempts were spent.';

  @override
  String get vaultLegacyTitle => 'Test-build data from before the PIN';

  @override
  String get vaultLegacyDetail =>
      'This data was written by a build that had no PIN yet. This build cannot move it under a PIN — on purpose: the move would be the riskiest code in the whole vault, for the sake of one launch on test data. There are no contacts there; a new identity will get a new fingerprint.';

  @override
  String get vaultLegacyAction => 'Start over with a PIN.';

  @override
  String get vaultPinSetupTitle => 'Set a PIN';

  @override
  String get vaultPinSetupDetail =>
      'Without the PIN the vault cannot be opened even on this phone, even unlocked. Every guess is checked by this phone\'s secure hardware, and guessing cannot be moved to other hardware. A forgotten PIN means losing all messages: there is no recovery yet.';

  @override
  String get vaultPinSetupAction =>
      '6 to 16 digits. 6 digits hold against a thief and a search; against a lab with root on this phone that is hours. 10 digits — years. Every extra digit makes guessing 10 times longer.';

  @override
  String get vaultPinMismatchTitle => 'The PINs did not match';

  @override
  String get vaultPinMismatchDetail =>
      'The second entry differs from the first. Nothing was saved.';

  @override
  String get vaultPinMismatchAction => 'Set the PIN again.';

  @override
  String get vaultWrongPinTitle => 'Wrong PIN';

  @override
  String vaultWrongPinDetail(int failures) {
    return 'Failures in a row: $failures. Five attempts are free; after that each one costs a wait: 30 s, 1 min, 5 min, 15 min, then an hour each.';
  }

  @override
  String get tryAgain => 'Try again.';

  @override
  String nextAttemptIn(String wait) {
    return 'Next attempt in $wait.';
  }

  @override
  String get vaultDelayedTitle => 'Too many failures';

  @override
  String vaultDelayedDetail(int failures) {
    return 'Failures in a row: $failures. While the wait runs, the PIN is not checked at all. Changing the clock does not help; a reboot starts the wait over.';
  }

  @override
  String get vaultRetryTitle => 'The vault is unavailable right now';

  @override
  String get vaultRetryFallback =>
      'The device\'s secure hardware did not answer.';

  @override
  String get vaultRetryAction =>
      'The data is intact, the attempt was not spent. Retry.';

  @override
  String get vaultUnavailableTitle => 'No vault on this platform';

  @override
  String get vaultUnavailableDetail =>
      'There is no hardware key store here, and putting the key in a file next to the data and calling that protection would be a lie. The build for this platform is frozen.';

  @override
  String get vaultLockedTitle => 'Locked';

  @override
  String get vaultLockedDetail =>
      'The vault key has been wiped from memory. It can be put together again only from the PIN, and every attempt is checked by this phone\'s secure hardware.';

  @override
  String get vaultLockedAction => 'Enter the PIN.';

  @override
  String get vaultSoftwareTitle => 'The key is not in hardware';

  @override
  String vaultSoftwareDetail(String level) {
    return 'The system reports: $level. So the PIN key lives not in the secure hardware but in the device\'s ordinary memory, and guessing the PIN is not bound by hardware. The app works, but the protection is weaker than promised.';
  }

  @override
  String get vaultOpenedTitle => 'Vault under PIN';

  @override
  String vaultOpenedDetail(String level) {
    return 'The system reports: $level. A copied data directory, a backup and a phone taken away unlocked are useless without the PIN. The PIN can be guessed only on this phone, one hardware check per attempt. With root on this phone 6 digits take hours to days, 8 — weeks to a year, 10 — years; the estimate for this phone is in the self-check. If the key is extracted from the hardware, only the length of the PIN holds.';
  }

  @override
  String vaultUnlockTook(int ms) {
    return 'Unlocking took $ms ms.';
  }

  @override
  String get levelSoftware => 'software';

  @override
  String get levelUnknownSecure => 'secure hardware, unspecified';

  @override
  String get levelUnknown => 'unknown';

  @override
  String waitSeconds(int n) {
    return '$n s';
  }

  @override
  String waitMinutes(int n) {
    return '$n min';
  }

  @override
  String waitHours(int n) {
    return '$n h';
  }

  @override
  String get pinRepeat => 'REPEAT THE PIN';

  @override
  String get pinNew => 'NEW PIN';

  @override
  String get pinEnter => 'ENTER THE PIN';

  @override
  String get pinLayoutNote =>
      'The layout is new on every attempt: watching the finger is useless, watching the screen is not. The screen is closed to screenshots and recording.';

  @override
  String get pinErase => 'Erase';

  @override
  String get pinDone => 'Done';

  @override
  String get retry => 'RETRY';

  @override
  String get startOver => 'START OVER';

  @override
  String get startOverWithPin => 'START OVER WITH A PIN';

  @override
  String get selfCheckTitle => 'SELF-CHECK';

  @override
  String get copyReport => 'Copy the whole report';

  @override
  String get reportCopied => 'Report copied';

  @override
  String get runAgain => 'Run again';

  @override
  String get allPassed => 'All passed.';

  @override
  String failedCount(int failed, int total) {
    return 'Failed: $failed of $total.';
  }

  @override
  String get checkPassed => '[ ok ]';

  @override
  String get checkFailed => '[FAIL]';

  @override
  String get dhtTitle => 'DHT MEASUREMENT';

  @override
  String get dhtBody =>
      'Whether messages can be delivered without a single server. The envelopes are test ones: random bytes, nothing about you. Put some, check your own a few hours later, and fetch the envelopes the desktop put. The log is kept between launches.';

  @override
  String dhtPut(int count) {
    return 'PUT $count';
  }

  @override
  String get dhtCheckOwn => 'CHECK OWN';

  @override
  String get dhtFetchDesktop => 'FETCH FROM DESKTOP';

  @override
  String get dhtClearLog => 'CLEAR LOG';

  @override
  String get platformTitle => 'PLATFORM';

  @override
  String get platformBody =>
      'There are no secrets here. This is what can be sent as a whole.';
}
