/// Which language the interface speaks.
///
/// The owner's decision: the language of the device if it is English or
/// Russian, otherwise English. The device's list of preferred languages is
/// walked in order, as Android itself does for apps: a phone set to
/// "Ukrainian, then Russian" gets Russian, one set to Ukrainian alone gets
/// English.
library;

import 'dart:ui' show Locale;

/// The language when none of the device's languages is supported.
const fallbackLocale = Locale('en');

/// A `localeListResolutionCallback` for [MaterialApp].
Locale chooseLocale(List<Locale>? preferred, Iterable<Locale> supported) {
  for (final wanted in preferred ?? const <Locale>[]) {
    for (final offered in supported) {
      if (offered.languageCode == wanted.languageCode) return offered;
    }
  }
  return fallbackLocale;
}
