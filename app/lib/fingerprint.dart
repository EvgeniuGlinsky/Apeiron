/// Fingerprint layout for the verification screen.
///
/// Split out and covered by tests on purpose. The split must be the same on
/// every screen: when verifying by voice, a shifting layout is a source of
/// errors, and an error here means a missed man in the middle.
/// This screen has no right to crash either — an unverified key is
/// equivalent to no protection at all.
library;

/// Splits fingerprint groups into rows of [per] each.
///
/// The last row may be incomplete. Empty input gives an empty result.
/// With `per <= 0` returns a single row with all groups instead of looping.
List<List<String>> fingerprintRows(List<String> groups, int per) {
  if (groups.isEmpty) return const [];
  if (per <= 0) return [List.of(groups)];
  return [
    for (var i = 0; i < groups.length; i += per)
      groups.sublist(i, i + per > groups.length ? groups.length : i + per),
  ];
}
