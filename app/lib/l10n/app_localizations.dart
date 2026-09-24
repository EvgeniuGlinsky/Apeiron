import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:intl/intl.dart' as intl;

import 'app_localizations_en.dart';
import 'app_localizations_ru.dart';

// ignore_for_file: type=lint

/// Callers can lookup localized strings with an instance of AppLocalizations
/// returned by `AppLocalizations.of(context)`.
///
/// Applications need to include `AppLocalizations.delegate()` in their app's
/// `localizationDelegates` list, and the locales they support in the app's
/// `supportedLocales` list. For example:
///
/// ```dart
/// import 'l10n/app_localizations.dart';
///
/// return MaterialApp(
///   localizationsDelegates: AppLocalizations.localizationsDelegates,
///   supportedLocales: AppLocalizations.supportedLocales,
///   home: MyApplicationHome(),
/// );
/// ```
///
/// ## Update pubspec.yaml
///
/// Please make sure to update your pubspec.yaml to include the following
/// packages:
///
/// ```yaml
/// dependencies:
///   # Internationalization support.
///   flutter_localizations:
///     sdk: flutter
///   intl: any # Use the pinned version from flutter_localizations
///
///   # Rest of dependencies
/// ```
///
/// ## iOS Applications
///
/// iOS applications define key application metadata, including supported
/// locales, in an Info.plist file that is built into the application bundle.
/// To configure the locales supported by your app, you’ll need to edit this
/// file.
///
/// First, open your project’s ios/Runner.xcworkspace Xcode workspace file.
/// Then, in the Project Navigator, open the Info.plist file under the Runner
/// project’s Runner folder.
///
/// Next, select the Information Property List item, select Add Item from the
/// Editor menu, then select Localizations from the pop-up menu.
///
/// Select and expand the newly-created Localizations item then, for each
/// locale your application supports, add a new item and select the locale
/// you wish to add from the pop-up menu in the Value field. This list should
/// be consistent with the languages listed in the AppLocalizations.supportedLocales
/// property.
abstract class AppLocalizations {
  AppLocalizations(String locale)
    : localeName = intl.Intl.canonicalizedLocale(locale.toString());

  final String localeName;

  static AppLocalizations of(BuildContext context) {
    return Localizations.of<AppLocalizations>(context, AppLocalizations)!;
  }

  static const LocalizationsDelegate<AppLocalizations> delegate =
      _AppLocalizationsDelegate();

  /// A list of this localizations delegate along with the default localizations
  /// delegates.
  ///
  /// Returns a list of localizations delegates containing this delegate along with
  /// GlobalMaterialLocalizations.delegate, GlobalCupertinoLocalizations.delegate,
  /// and GlobalWidgetsLocalizations.delegate.
  ///
  /// Additional delegates can be added by appending to this list in
  /// MaterialApp. This list does not have to be used at all if a custom list
  /// of delegates is preferred or required.
  static const List<LocalizationsDelegate<dynamic>> localizationsDelegates =
      <LocalizationsDelegate<dynamic>>[
        delegate,
        GlobalMaterialLocalizations.delegate,
        GlobalCupertinoLocalizations.delegate,
        GlobalWidgetsLocalizations.delegate,
      ];

  /// A list of this localizations delegate's supported locales.
  static const List<Locale> supportedLocales = <Locale>[
    Locale('en'),
    Locale('ru'),
  ];

  /// No description provided for @selfCheckTooltip.
  ///
  /// In en, this message translates to:
  /// **'Self-check'**
  String get selfCheckTooltip;

  /// No description provided for @lockTooltip.
  ///
  /// In en, this message translates to:
  /// **'Lock'**
  String get lockTooltip;

  /// No description provided for @noIdentityTitle.
  ///
  /// In en, this message translates to:
  /// **'NO IDENTITY YET'**
  String get noIdentityTitle;

  /// No description provided for @noIdentityBody.
  ///
  /// In en, this message translates to:
  /// **'The vault on this device has no identity yet. The device keys and the identity log are created together with it — apart they are meaningless.'**
  String get noIdentityBody;

  /// No description provided for @createIdentity.
  ///
  /// In en, this message translates to:
  /// **'CREATE IDENTITY'**
  String get createIdentity;

  /// No description provided for @fingerprintTitle.
  ///
  /// In en, this message translates to:
  /// **'YOUR FINGERPRINT'**
  String get fingerprintTitle;

  /// No description provided for @fingerprintBody.
  ///
  /// In en, this message translates to:
  /// **'It identifies you. It is not what you compare with someone: for that, open the chat and tap Verify — the safety number there is one and the same for the two of you.'**
  String get fingerprintBody;

  /// No description provided for @notVerified.
  ///
  /// In en, this message translates to:
  /// **'NOT VERIFIED WITH ANYONE'**
  String get notVerified;

  /// No description provided for @keySigning.
  ///
  /// In en, this message translates to:
  /// **'ED25519 · SIGNING'**
  String get keySigning;

  /// No description provided for @keyAgreement.
  ///
  /// In en, this message translates to:
  /// **'X25519 · AGREEMENT'**
  String get keyAgreement;

  /// No description provided for @copy.
  ///
  /// In en, this message translates to:
  /// **'Copy'**
  String get copy;

  /// No description provided for @copied.
  ///
  /// In en, this message translates to:
  /// **'Copied'**
  String get copied;

  /// No description provided for @trueNowTitle.
  ///
  /// In en, this message translates to:
  /// **'WHAT IS ALREADY TRUE HERE'**
  String get trueNowTitle;

  /// No description provided for @trueNowBody.
  ///
  /// In en, this message translates to:
  /// **'Secret keys never leave Rust — only the public halves and the fingerprint came here. {explanation}'**
  String trueNowBody(String explanation);

  /// No description provided for @notYetTitle.
  ///
  /// In en, this message translates to:
  /// **'HOW MESSAGES TRAVEL'**
  String get notYetTitle;

  /// No description provided for @notYetBody.
  ///
  /// In en, this message translates to:
  /// **'Messages travel through Mainline DHT, the network BitTorrent uses: nobody owns it and no server of ours is in the way. The content is protected; who talks to whom is visible to the network.'**
  String get notYetBody;

  /// No description provided for @lockExplainMobile.
  ///
  /// In en, this message translates to:
  /// **'When the app goes to the background, the identity is destroyed together with the keys.'**
  String get lockExplainMobile;

  /// No description provided for @lockExplainDesktop.
  ///
  /// In en, this message translates to:
  /// **'The identity is destroyed together with the keys when the window is minimised or there is no activity for {minutes, plural, one{1 minute} other{{minutes} minutes}}. Switching to another window leaves it alone: on a desktop that happens too often to mean anything.'**
  String lockExplainDesktop(int minutes);

  /// No description provided for @lockExplainDesktopNoTimeout.
  ///
  /// In en, this message translates to:
  /// **'The identity is destroyed together with the keys when the window is minimised. Switching to another window leaves it alone: on a desktop that happens too often to mean anything.'**
  String get lockExplainDesktopNoTimeout;

  /// No description provided for @keyNotExported.
  ///
  /// In en, this message translates to:
  /// **'The key lived in this phone\'s secure hardware and was never exported from it — that was the point — so there is nowhere to get it back from.'**
  String get keyNotExported;

  /// No description provided for @startOverAction.
  ///
  /// In en, this message translates to:
  /// **'Start over. Earlier messages will not come back.'**
  String get startOverAction;

  /// No description provided for @vaultKeyGoneTitle.
  ///
  /// In en, this message translates to:
  /// **'This phone\'s key is gone'**
  String get vaultKeyGoneTitle;

  /// No description provided for @vaultKeyGoneDetail.
  ///
  /// In en, this message translates to:
  /// **'The vault key has disappeared from the secure hardware. This happens after a firmware update, when the screen lock is removed, and when data is restored from a backup.'**
  String get vaultKeyGoneDetail;

  /// No description provided for @vaultKeyMismatchTitle.
  ///
  /// In en, this message translates to:
  /// **'The key in the secure hardware is the wrong one'**
  String get vaultKeyMismatchTitle;

  /// No description provided for @vaultKeyMismatchDetail.
  ///
  /// In en, this message translates to:
  /// **'There is a key in the secure hardware, but not the one the vault was made with. This is not a wrong PIN: no attempts were spent.'**
  String get vaultKeyMismatchDetail;

  /// No description provided for @vaultLegacyTitle.
  ///
  /// In en, this message translates to:
  /// **'Test-build data from before the PIN'**
  String get vaultLegacyTitle;

  /// No description provided for @vaultLegacyDetail.
  ///
  /// In en, this message translates to:
  /// **'This data was written by a build that had no PIN yet. This build cannot move it under a PIN — on purpose: the move would be the riskiest code in the whole vault, for the sake of one launch on test data. There are no contacts there; a new identity will get a new fingerprint.'**
  String get vaultLegacyDetail;

  /// No description provided for @vaultLegacyAction.
  ///
  /// In en, this message translates to:
  /// **'Start over with a PIN.'**
  String get vaultLegacyAction;

  /// No description provided for @vaultPinSetupTitle.
  ///
  /// In en, this message translates to:
  /// **'Set a PIN'**
  String get vaultPinSetupTitle;

  /// No description provided for @vaultPinSetupDetail.
  ///
  /// In en, this message translates to:
  /// **'Without the PIN the vault cannot be opened even on this phone, even unlocked. Every guess is checked by this phone\'s secure hardware, and guessing cannot be moved to other hardware. A forgotten PIN means losing all messages: there is no recovery yet.'**
  String get vaultPinSetupDetail;

  /// No description provided for @vaultPinSetupAction.
  ///
  /// In en, this message translates to:
  /// **'6 to 16 digits. 6 digits hold against a thief and a search; against a lab with root on this phone that is hours. 10 digits — years. Every extra digit makes guessing 10 times longer.'**
  String get vaultPinSetupAction;

  /// No description provided for @vaultPinMismatchTitle.
  ///
  /// In en, this message translates to:
  /// **'The PINs did not match'**
  String get vaultPinMismatchTitle;

  /// No description provided for @vaultPinMismatchDetail.
  ///
  /// In en, this message translates to:
  /// **'The second entry differs from the first. Nothing was saved.'**
  String get vaultPinMismatchDetail;

  /// No description provided for @vaultPinMismatchAction.
  ///
  /// In en, this message translates to:
  /// **'Set the PIN again.'**
  String get vaultPinMismatchAction;

  /// No description provided for @vaultWrongPinTitle.
  ///
  /// In en, this message translates to:
  /// **'Wrong PIN'**
  String get vaultWrongPinTitle;

  /// No description provided for @vaultWrongPinDetail.
  ///
  /// In en, this message translates to:
  /// **'Failures in a row: {failures}. Five attempts are free; after that each one costs a wait: 30 s, 1 min, 5 min, 15 min, then an hour each.'**
  String vaultWrongPinDetail(int failures);

  /// No description provided for @tryAgain.
  ///
  /// In en, this message translates to:
  /// **'Try again.'**
  String get tryAgain;

  /// No description provided for @nextAttemptIn.
  ///
  /// In en, this message translates to:
  /// **'Next attempt in {wait}.'**
  String nextAttemptIn(String wait);

  /// No description provided for @vaultDelayedTitle.
  ///
  /// In en, this message translates to:
  /// **'Too many failures'**
  String get vaultDelayedTitle;

  /// No description provided for @vaultDelayedDetail.
  ///
  /// In en, this message translates to:
  /// **'Failures in a row: {failures}. While the wait runs, the PIN is not checked at all. Changing the clock does not help; a reboot starts the wait over.'**
  String vaultDelayedDetail(int failures);

  /// No description provided for @vaultRetryTitle.
  ///
  /// In en, this message translates to:
  /// **'The vault is unavailable right now'**
  String get vaultRetryTitle;

  /// No description provided for @vaultRetryFallback.
  ///
  /// In en, this message translates to:
  /// **'The device\'s secure hardware did not answer.'**
  String get vaultRetryFallback;

  /// No description provided for @vaultRetryAction.
  ///
  /// In en, this message translates to:
  /// **'The data is intact, the attempt was not spent. Retry.'**
  String get vaultRetryAction;

  /// No description provided for @vaultUnavailableTitle.
  ///
  /// In en, this message translates to:
  /// **'No vault on this platform'**
  String get vaultUnavailableTitle;

  /// No description provided for @vaultUnavailableDetail.
  ///
  /// In en, this message translates to:
  /// **'There is no hardware key store here, and putting the key in a file next to the data and calling that protection would be a lie. The build for this platform is frozen.'**
  String get vaultUnavailableDetail;

  /// No description provided for @vaultLockedTitle.
  ///
  /// In en, this message translates to:
  /// **'Locked'**
  String get vaultLockedTitle;

  /// No description provided for @vaultLockedDetail.
  ///
  /// In en, this message translates to:
  /// **'The vault key has been wiped from memory. It can be put together again only from the PIN, and every attempt is checked by this phone\'s secure hardware.'**
  String get vaultLockedDetail;

  /// No description provided for @vaultLockedAction.
  ///
  /// In en, this message translates to:
  /// **'Enter the PIN.'**
  String get vaultLockedAction;

  /// No description provided for @vaultSoftwareTitle.
  ///
  /// In en, this message translates to:
  /// **'The key is not in hardware'**
  String get vaultSoftwareTitle;

  /// No description provided for @vaultSoftwareDetail.
  ///
  /// In en, this message translates to:
  /// **'The system reports: {level}. So the PIN key lives not in the secure hardware but in the device\'s ordinary memory, and guessing the PIN is not bound by hardware. The app works, but the protection is weaker than promised.'**
  String vaultSoftwareDetail(String level);

  /// No description provided for @vaultOpenedTitle.
  ///
  /// In en, this message translates to:
  /// **'Vault under PIN'**
  String get vaultOpenedTitle;

  /// No description provided for @vaultOpenedDetail.
  ///
  /// In en, this message translates to:
  /// **'The system reports: {level}. A copied data directory, a backup and a phone taken away unlocked are useless without the PIN. The PIN can be guessed only on this phone, one hardware check per attempt. With root on this phone 6 digits take hours to days, 8 — weeks to a year, 10 — years; the estimate for this phone is in the self-check. If the key is extracted from the hardware, only the length of the PIN holds.'**
  String vaultOpenedDetail(String level);

  /// No description provided for @vaultUnlockTook.
  ///
  /// In en, this message translates to:
  /// **'Unlocking took {ms} ms.'**
  String vaultUnlockTook(int ms);

  /// No description provided for @levelSoftware.
  ///
  /// In en, this message translates to:
  /// **'software'**
  String get levelSoftware;

  /// No description provided for @levelUnknownSecure.
  ///
  /// In en, this message translates to:
  /// **'secure hardware, unspecified'**
  String get levelUnknownSecure;

  /// No description provided for @levelUnknown.
  ///
  /// In en, this message translates to:
  /// **'unknown'**
  String get levelUnknown;

  /// No description provided for @waitSeconds.
  ///
  /// In en, this message translates to:
  /// **'{n} s'**
  String waitSeconds(int n);

  /// No description provided for @waitMinutes.
  ///
  /// In en, this message translates to:
  /// **'{n} min'**
  String waitMinutes(int n);

  /// No description provided for @waitHours.
  ///
  /// In en, this message translates to:
  /// **'{n} h'**
  String waitHours(int n);

  /// No description provided for @pinRepeat.
  ///
  /// In en, this message translates to:
  /// **'REPEAT THE PIN'**
  String get pinRepeat;

  /// No description provided for @pinNew.
  ///
  /// In en, this message translates to:
  /// **'NEW PIN'**
  String get pinNew;

  /// No description provided for @pinEnter.
  ///
  /// In en, this message translates to:
  /// **'ENTER THE PIN'**
  String get pinEnter;

  /// No description provided for @pinLayoutNote.
  ///
  /// In en, this message translates to:
  /// **'The layout is new on every attempt: watching the finger is useless, watching the screen is not. The screen is closed to screenshots and recording.'**
  String get pinLayoutNote;

  /// No description provided for @pinErase.
  ///
  /// In en, this message translates to:
  /// **'Erase'**
  String get pinErase;

  /// No description provided for @pinDone.
  ///
  /// In en, this message translates to:
  /// **'Done'**
  String get pinDone;

  /// No description provided for @retry.
  ///
  /// In en, this message translates to:
  /// **'RETRY'**
  String get retry;

  /// No description provided for @startOver.
  ///
  /// In en, this message translates to:
  /// **'START OVER'**
  String get startOver;

  /// No description provided for @startOverWithPin.
  ///
  /// In en, this message translates to:
  /// **'START OVER WITH A PIN'**
  String get startOverWithPin;

  /// No description provided for @selfCheckTitle.
  ///
  /// In en, this message translates to:
  /// **'SELF-CHECK'**
  String get selfCheckTitle;

  /// No description provided for @copyReport.
  ///
  /// In en, this message translates to:
  /// **'Copy the whole report'**
  String get copyReport;

  /// No description provided for @reportCopied.
  ///
  /// In en, this message translates to:
  /// **'Report copied'**
  String get reportCopied;

  /// No description provided for @runAgain.
  ///
  /// In en, this message translates to:
  /// **'Run again'**
  String get runAgain;

  /// No description provided for @allPassed.
  ///
  /// In en, this message translates to:
  /// **'All passed.'**
  String get allPassed;

  /// No description provided for @failedCount.
  ///
  /// In en, this message translates to:
  /// **'Failed: {failed} of {total}.'**
  String failedCount(int failed, int total);

  /// No description provided for @checkPassed.
  ///
  /// In en, this message translates to:
  /// **'[ ok ]'**
  String get checkPassed;

  /// No description provided for @checkFailed.
  ///
  /// In en, this message translates to:
  /// **'[FAIL]'**
  String get checkFailed;

  /// No description provided for @dhtTitle.
  ///
  /// In en, this message translates to:
  /// **'DHT MEASUREMENT'**
  String get dhtTitle;

  /// No description provided for @dhtBody.
  ///
  /// In en, this message translates to:
  /// **'Whether messages can be delivered without a single server. The envelopes are test ones: random bytes, nothing about you. Put some, check your own a few hours later, and fetch the envelopes the desktop put. The log is kept between launches.'**
  String get dhtBody;

  /// No description provided for @dhtPut.
  ///
  /// In en, this message translates to:
  /// **'PUT {count}'**
  String dhtPut(int count);

  /// No description provided for @dhtCheckOwn.
  ///
  /// In en, this message translates to:
  /// **'CHECK OWN'**
  String get dhtCheckOwn;

  /// No description provided for @dhtFetchDesktop.
  ///
  /// In en, this message translates to:
  /// **'FETCH FROM DESKTOP'**
  String get dhtFetchDesktop;

  /// No description provided for @dhtClearLog.
  ///
  /// In en, this message translates to:
  /// **'CLEAR LOG'**
  String get dhtClearLog;

  /// No description provided for @platformTitle.
  ///
  /// In en, this message translates to:
  /// **'PLATFORM'**
  String get platformTitle;

  /// No description provided for @platformBody.
  ///
  /// In en, this message translates to:
  /// **'There are no secrets here. This is what can be sent as a whole.'**
  String get platformBody;

  /// No description provided for @pinChoiceTitle.
  ///
  /// In en, this message translates to:
  /// **'HOW THE PIN WILL BE'**
  String get pinChoiceTitle;

  /// No description provided for @pinChoiceLength.
  ///
  /// In en, this message translates to:
  /// **'LENGTH'**
  String get pinChoiceLength;

  /// No description provided for @pinDigits.
  ///
  /// In en, this message translates to:
  /// **'{count, plural, one{{count} digit} other{{count} digits}}'**
  String pinDigits(int count);

  /// No description provided for @pinChoiceKeyboard.
  ///
  /// In en, this message translates to:
  /// **'KEYBOARD'**
  String get pinChoiceKeyboard;

  /// No description provided for @pinKeyboardScrambled.
  ///
  /// In en, this message translates to:
  /// **'Scrambled'**
  String get pinKeyboardScrambled;

  /// No description provided for @pinKeyboardOrdered.
  ///
  /// In en, this message translates to:
  /// **'Usual'**
  String get pinKeyboardOrdered;

  /// No description provided for @pinRecommendedTag.
  ///
  /// In en, this message translates to:
  /// **'recommended'**
  String get pinRecommendedTag;

  /// No description provided for @pinChoiceRecommend.
  ///
  /// In en, this message translates to:
  /// **'We recommend 8 digits and the scrambled keyboard: it is the most reliable choice.'**
  String get pinChoiceRecommend;

  /// No description provided for @pinChoiceHonest.
  ///
  /// In en, this message translates to:
  /// **'Someone who takes this phone and gets root on it can try PINs in its secure chip: 4 digits fall in minutes, 6 in about half a day, 8 in about a month. The scrambled keyboard hides the PIN from someone who sees only your finger.'**
  String get pinChoiceHonest;

  /// No description provided for @pinChoiceContinue.
  ///
  /// In en, this message translates to:
  /// **'CONTINUE'**
  String get pinChoiceContinue;

  /// No description provided for @pinOrderedNote.
  ///
  /// In en, this message translates to:
  /// **'The screen is closed to screenshots and recording.'**
  String get pinOrderedNote;

  /// No description provided for @settingsTooltip.
  ///
  /// In en, this message translates to:
  /// **'Settings'**
  String get settingsTooltip;

  /// No description provided for @settingsTitle.
  ///
  /// In en, this message translates to:
  /// **'SETTINGS'**
  String get settingsTitle;

  /// No description provided for @settingsPin.
  ///
  /// In en, this message translates to:
  /// **'PIN'**
  String get settingsPin;

  /// No description provided for @settingsKeyboard.
  ///
  /// In en, this message translates to:
  /// **'Scrambled keyboard'**
  String get settingsKeyboard;

  /// No description provided for @settingsKeyboardNote.
  ///
  /// In en, this message translates to:
  /// **'A new layout for every attempt. Recommended.'**
  String get settingsKeyboardNote;

  /// No description provided for @settingsChangePin.
  ///
  /// In en, this message translates to:
  /// **'Change PIN'**
  String get settingsChangePin;

  /// No description provided for @settingsChangePinNote.
  ///
  /// In en, this message translates to:
  /// **'Nothing is re-encrypted: only the key of the data is sealed under the new PIN.'**
  String get settingsChangePinNote;

  /// No description provided for @settingsIdentity.
  ///
  /// In en, this message translates to:
  /// **'MY IDENTITY'**
  String get settingsIdentity;

  /// No description provided for @settingsSelfCheck.
  ///
  /// In en, this message translates to:
  /// **'Self-check'**
  String get settingsSelfCheck;

  /// No description provided for @changePinTitle.
  ///
  /// In en, this message translates to:
  /// **'CHANGE PIN'**
  String get changePinTitle;

  /// No description provided for @pinCurrent.
  ///
  /// In en, this message translates to:
  /// **'CURRENT PIN'**
  String get pinCurrent;

  /// No description provided for @pinChanged.
  ///
  /// In en, this message translates to:
  /// **'The PIN is changed'**
  String get pinChanged;

  /// No description provided for @chatsEmptyTitle.
  ///
  /// In en, this message translates to:
  /// **'NO CONVERSATIONS YET'**
  String get chatsEmptyTitle;

  /// No description provided for @chatsEmptyBody.
  ///
  /// In en, this message translates to:
  /// **'Invite someone, or accept an invitation someone sent you. Messages go through the BitTorrent DHT: no server of ours holds them.'**
  String get chatsEmptyBody;

  /// No description provided for @invite.
  ///
  /// In en, this message translates to:
  /// **'INVITE'**
  String get invite;

  /// No description provided for @acceptInvitation.
  ///
  /// In en, this message translates to:
  /// **'ACCEPT AN INVITATION'**
  String get acceptInvitation;

  /// No description provided for @invitationsWaiting.
  ///
  /// In en, this message translates to:
  /// **'INVITATIONS WAITING FOR AN ANSWER'**
  String get invitationsWaiting;

  /// No description provided for @invitationUntil.
  ///
  /// In en, this message translates to:
  /// **'until {date}'**
  String invitationUntil(String date);

  /// No description provided for @contactWaiting.
  ///
  /// In en, this message translates to:
  /// **'waiting for them to accept'**
  String get contactWaiting;

  /// No description provided for @contactNotAccepted.
  ///
  /// In en, this message translates to:
  /// **'the invitation was not accepted'**
  String get contactNotAccepted;

  /// No description provided for @contactTaken.
  ///
  /// In en, this message translates to:
  /// **'someone else answered the invitation first'**
  String get contactTaken;

  /// No description provided for @contactDamaged.
  ///
  /// In en, this message translates to:
  /// **'the conversation is damaged'**
  String get contactDamaged;

  /// No description provided for @contactVerified.
  ///
  /// In en, this message translates to:
  /// **'verified'**
  String get contactVerified;

  /// No description provided for @contactNotVerified.
  ///
  /// In en, this message translates to:
  /// **'not verified'**
  String get contactNotVerified;

  /// No description provided for @inviteTitle.
  ///
  /// In en, this message translates to:
  /// **'INVITATION'**
  String get inviteTitle;

  /// No description provided for @inviteName.
  ///
  /// In en, this message translates to:
  /// **'Who it is for (the name you will see)'**
  String get inviteName;

  /// No description provided for @inviteCreate.
  ///
  /// In en, this message translates to:
  /// **'CREATE INVITATION'**
  String get inviteCreate;

  /// No description provided for @inviteBody.
  ///
  /// In en, this message translates to:
  /// **'Send this text through any channel. Whoever reads it on the way can answer in the person\'s name, so compare the safety number with them afterwards — in person or by voice.'**
  String get inviteBody;

  /// No description provided for @inviteExpires.
  ///
  /// In en, this message translates to:
  /// **'Valid until {date}.'**
  String inviteExpires(String date);

  /// No description provided for @inviteWithdraw.
  ///
  /// In en, this message translates to:
  /// **'WITHDRAW'**
  String get inviteWithdraw;

  /// No description provided for @acceptPaste.
  ///
  /// In en, this message translates to:
  /// **'Invitation text (apeiron:…)'**
  String get acceptPaste;

  /// No description provided for @acceptName.
  ///
  /// In en, this message translates to:
  /// **'Name for this contact'**
  String get acceptName;

  /// No description provided for @acceptFirst.
  ///
  /// In en, this message translates to:
  /// **'First message (optional)'**
  String get acceptFirst;

  /// No description provided for @acceptButton.
  ///
  /// In en, this message translates to:
  /// **'ACCEPT'**
  String get acceptButton;

  /// No description provided for @acceptNote.
  ///
  /// In en, this message translates to:
  /// **'The contact shows as waiting until the person who invited you takes your answer.'**
  String get acceptNote;

  /// No description provided for @messageHint.
  ///
  /// In en, this message translates to:
  /// **'Message'**
  String get messageHint;

  /// No description provided for @send.
  ///
  /// In en, this message translates to:
  /// **'Send'**
  String get send;

  /// No description provided for @msgQueued.
  ///
  /// In en, this message translates to:
  /// **'not sent yet'**
  String get msgQueued;

  /// No description provided for @msgSent.
  ///
  /// In en, this message translates to:
  /// **'sent'**
  String get msgSent;

  /// No description provided for @msgDelivered.
  ///
  /// In en, this message translates to:
  /// **'delivered'**
  String get msgDelivered;

  /// No description provided for @msgNotDelivered.
  ///
  /// In en, this message translates to:
  /// **'not delivered'**
  String get msgNotDelivered;

  /// No description provided for @msgAddressTaken.
  ///
  /// In en, this message translates to:
  /// **'not delivered: the address was taken'**
  String get msgAddressTaken;

  /// No description provided for @msgLost.
  ///
  /// In en, this message translates to:
  /// **'Some of their messages were lost'**
  String get msgLost;

  /// No description provided for @chatWaitingNote.
  ///
  /// In en, this message translates to:
  /// **'They have not accepted yet. You can write: the messages will wait.'**
  String get chatWaitingNote;

  /// No description provided for @chatClosedNote.
  ///
  /// In en, this message translates to:
  /// **'Nothing more will come of this conversation.'**
  String get chatClosedNote;

  /// No description provided for @chatEmpty.
  ///
  /// In en, this message translates to:
  /// **'No messages yet.'**
  String get chatEmpty;

  /// No description provided for @verifyAction.
  ///
  /// In en, this message translates to:
  /// **'Verify'**
  String get verifyAction;

  /// No description provided for @verifyTitle.
  ///
  /// In en, this message translates to:
  /// **'SAFETY NUMBER'**
  String get verifyTitle;

  /// No description provided for @verifyBody.
  ///
  /// In en, this message translates to:
  /// **'One number for the two of you: {name} sees exactly these digits on this screen. It is not your fingerprint from Settings.'**
  String verifyBody(String name);

  /// No description provided for @verifyConfirm.
  ///
  /// In en, this message translates to:
  /// **'THE NUMBERS MATCH'**
  String get verifyConfirm;

  /// No description provided for @verifyUndo.
  ///
  /// In en, this message translates to:
  /// **'TAKE THE MARK BACK'**
  String get verifyUndo;

  /// No description provided for @verifiedByYou.
  ///
  /// In en, this message translates to:
  /// **'VERIFIED BY YOU'**
  String get verifiedByYou;

  /// No description provided for @verifyStep1.
  ///
  /// In en, this message translates to:
  /// **'Call {name} — by voice, not through the channel the invitation went through.'**
  String verifyStep1(String name);

  /// No description provided for @verifyStep2.
  ///
  /// In en, this message translates to:
  /// **'Both open this screen. You read the first row aloud, {name} reads the second.'**
  String verifyStep2(String name);

  /// No description provided for @verifyStep3.
  ///
  /// In en, this message translates to:
  /// **'Every digit the same — they match. Even one differs — they do not.'**
  String get verifyStep3;

  /// No description provided for @verifyMismatch.
  ///
  /// In en, this message translates to:
  /// **'THEY DO NOT MATCH'**
  String get verifyMismatch;

  /// No description provided for @mismatchTitle.
  ///
  /// In en, this message translates to:
  /// **'THE NUMBERS DO NOT MATCH'**
  String get mismatchTitle;

  /// No description provided for @mismatchBody.
  ///
  /// In en, this message translates to:
  /// **'Until this is settled, write nothing you would not say to a stranger: someone may be between you. Find out whose key is not the one the other holds — compare the two fingerprints below with what each of you sees in Settings → My identity.'**
  String get mismatchBody;

  /// No description provided for @mismatchTheirs.
  ///
  /// In en, this message translates to:
  /// **'The fingerprint of {name}, as this phone holds it. {name} must see the same in their settings:'**
  String mismatchTheirs(String name);

  /// No description provided for @mismatchMine.
  ///
  /// In en, this message translates to:
  /// **'Your fingerprint. {name} must hold the same for you:'**
  String mismatchMine(String name);

  /// No description provided for @mismatchVerdict.
  ///
  /// In en, this message translates to:
  /// **'A fingerprint differs — a key was replaced: do not trust this conversation. Both are the same and the numbers still differ — that is a fault of the app: send screenshots of both screens.'**
  String get mismatchVerdict;

  /// No description provided for @mismatchBack.
  ///
  /// In en, this message translates to:
  /// **'BACK TO THE NUMBER'**
  String get mismatchBack;

  /// No description provided for @msgRead.
  ///
  /// In en, this message translates to:
  /// **'read'**
  String get msgRead;

  /// No description provided for @previewMine.
  ///
  /// In en, this message translates to:
  /// **'You: {text}'**
  String previewMine(String text);

  /// No description provided for @settingsReadReceipts.
  ///
  /// In en, this message translates to:
  /// **'Read receipts'**
  String get settingsReadReceipts;

  /// No description provided for @settingsReadReceiptsNote.
  ///
  /// In en, this message translates to:
  /// **'The person you write to learns when you were shown their message, and you learn the same about yours. Off, neither is sent nor shown. The network learns little new: an open chat already shows in how often the app asks.'**
  String get settingsReadReceiptsNote;
}

class _AppLocalizationsDelegate
    extends LocalizationsDelegate<AppLocalizations> {
  const _AppLocalizationsDelegate();

  @override
  Future<AppLocalizations> load(Locale locale) {
    return SynchronousFuture<AppLocalizations>(lookupAppLocalizations(locale));
  }

  @override
  bool isSupported(Locale locale) =>
      <String>['en', 'ru'].contains(locale.languageCode);

  @override
  bool shouldReload(_AppLocalizationsDelegate old) => false;
}

AppLocalizations lookupAppLocalizations(Locale locale) {
  // Lookup logic when only language code is specified.
  switch (locale.languageCode) {
    case 'en':
      return AppLocalizationsEn();
    case 'ru':
      return AppLocalizationsRu();
  }

  throw FlutterError(
    'AppLocalizations.delegate failed to load unsupported locale "$locale". This is likely '
    'an issue with the localizations generation tool. Please file an issue '
    'on GitHub with a reproducible sample app and the gen-l10n configuration '
    'that was used.',
  );
}
