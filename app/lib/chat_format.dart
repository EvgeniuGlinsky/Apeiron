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

String _two(int n) => n.toString().padLeft(2, '0');
