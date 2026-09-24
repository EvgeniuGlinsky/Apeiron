/// The interface speaks English and Russian, and both say the same thing.
///
/// gen-l10n does not fail on a key missing from a translation: it silently
/// shows the English line inside a Russian screen. And no tool at all notices
/// a translation that promises more than the original. Both are checked here.
library;

import 'dart:convert';
import 'dart:io';

import 'package:apeiron/l10n/app_localizations.dart';
import 'package:apeiron/locale_choice.dart';
import 'package:flutter/widgets.dart' show Locale;
import 'package:flutter_test/flutter_test.dart';

/// Messages of one ARB file, without the `@` metadata.
Map<String, String> messages(String lang) {
  final json =
      jsonDecode(File('lib/l10n/app_$lang.arb').readAsStringSync())
          as Map<String, dynamic>;
  return {
    for (final e in json.entries)
      if (!e.key.startsWith('@')) e.key: e.value as String,
  };
}

/// Placeholder names used in a message, plural variables included.
Set<String> placeholders(String message) =>
    RegExp(r'\{(\w+)[,}]').allMatches(message).map((m) => m.group(1)!).toSet();

/// Wordings the interface must never use, from the table in
/// `docs/threat-log.md` ("Wordings that must not be used"), in both languages
/// and in the forms a translator would reach for. Checked in every file: a
/// Russian word in the English file is as much a slip as the other way round.
const forbidden = [
  // "Protects you from the state"
  'protects you from the state',
  'protect you from the state',
  'защищает от государства',
  'защитит от государства',
  // "Complete anonymity"
  'anonymity',
  'anonymous',
  'анонимн',
  // "No one will know you use it"
  'no one will know',
  'nobody will know',
  'никто не узнает',
  // "Military-grade encryption"
  'military-grade',
  'military grade',
  'военного уровня',
  'военного класса',
  // "A hidden account will protect you during a search"
  'hidden account will protect',
  'скрытый аккаунт защитит',
  // Not in the table, but the same kind of promise.
  'unbreakable',
  'невзламываем',
];

void main() {
  final languages = AppLocalizations.supportedLocales
      .map((l) => l.languageCode)
      .toList();
  final english = messages('en');

  test('the interface speaks exactly English and Russian', () {
    expect(languages.toSet(), {'en', 'ru'});
  });

  for (final lang in languages) {
    final own = messages(lang);

    group('[$lang]', () {
      test('has every message of the template and nothing extra', () {
        expect(
          own.keys.toSet().difference(english.keys.toSet()),
          isEmpty,
          reason: 'messages that the template does not know',
        );
        expect(
          english.keys.toSet().difference(own.keys.toSet()),
          isEmpty,
          reason: 'missing messages would show in English',
        );
      });

      test('uses the same placeholders as the template', () {
        for (final key in english.keys) {
          expect(
            placeholders(own[key] ?? ''),
            placeholders(english[key]!),
            reason: '"$key": a lost placeholder drops a number from the text',
          );
        }
      });

      test('no message is empty', () {
        for (final e in own.entries) {
          expect(e.value.trim(), isNotEmpty, reason: e.key);
        }
      });

      test('never uses a forbidden wording', () {
        for (final e in own.entries) {
          final text = e.value.toLowerCase();
          for (final phrase in forbidden) {
            expect(
              text,
              isNot(contains(phrase)),
              reason: '"${e.key}" promises what is not true: "$phrase"',
            );
          }
        }
      });
    });
  }

  test('no Russian message is an English line left untranslated', () {
    final cyrillic = RegExp('[А-Яа-яЁё]');
    for (final e in messages('ru').entries) {
      expect(
        cyrillic.hasMatch(e.value),
        isTrue,
        reason: '"${e.key}" in the Russian file has no Russian in it',
      );
    }
  });

  group('the language is chosen by the device', () {
    const supported = AppLocalizations.supportedLocales;

    test('Russian for a Russian device', () {
      expect(
        chooseLocale([const Locale('ru', 'RU')], supported),
        const Locale('ru'),
      );
    });

    test('English for an English device', () {
      expect(
        chooseLocale([const Locale('en', 'GB')], supported),
        const Locale('en'),
      );
    });

    test('English when no language of the device is supported', () {
      expect(chooseLocale([const Locale('uk')], supported), fallbackLocale);
      expect(chooseLocale([const Locale('de')], supported), fallbackLocale);
      expect(chooseLocale(const [], supported), fallbackLocale);
      expect(chooseLocale(null, supported), fallbackLocale);
    });

    test('the order of the device languages is respected', () {
      expect(
        chooseLocale([const Locale('uk'), const Locale('ru')], supported),
        const Locale('ru'),
      );
      expect(
        chooseLocale([const Locale('en'), const Locale('ru')], supported),
        const Locale('en'),
      );
    });

    test('the fallback is English', () {
      expect(fallbackLocale, const Locale('en'));
    });
  });
}
