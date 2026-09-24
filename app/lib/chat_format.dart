/// Small formatting for the conversation screens, kept apart so it can be tested without Rust.
library;

/// The date of a UTC day counted from 1970-01-01, as `dd.mm.yyyy`.
String dayDate(int day) {
  final d = DateTime.utc(1970).add(Duration(days: day));
  return '${_two(d.day)}.${_two(d.month)}.${d.year}';
}

/// The local time of Unix seconds, as `hh:mm`.
String clockTime(int unixSeconds) {
  final d = DateTime.fromMillisecondsSinceEpoch(unixSeconds * 1000);
  return '${_two(d.hour)}:${_two(d.minute)}';
}

/// When the list's newest entry was: the time today, the day this year, the date before that.
String listTime(int unixSeconds, DateTime now) {
  final d = DateTime.fromMillisecondsSinceEpoch(unixSeconds * 1000);
  if (d.year == now.year && d.month == now.month && d.day == now.day) {
    return clockTime(unixSeconds);
  }
  final day = '${_two(d.day)}.${_two(d.month)}';
  return d.year == now.year ? day : '$day.${d.year}';
}

String _two(int n) => n.toString().padLeft(2, '0');
