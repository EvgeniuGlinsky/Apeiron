import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'src/rust/api/probe.dart';
import 'src/rust/api/vault.dart';
import 'theme/tokens.dart';

/// Self-check screen: a "passed / FAILED" list and a platform report.
///
/// It exists because there is only one check on a live device. The screen
/// must answer every question at once, not just the one someone thought to
/// ask — so the checks are shown together with full diagnostics that can be
/// photographed or copied in one piece.
///
/// The checks destroy nothing. There is no erasure here: it destroys the
/// owner's data, and must not be invoked under the guise of a check.
class CheckScreen extends StatefulWidget {
  const CheckScreen({super.key});

  @override
  State<CheckScreen> createState() => _CheckScreenState();
}

class _CheckScreenState extends State<CheckScreen> {
  List<CheckLine>? _checks;
  String? _diagnostics;
  String? _error;
  bool _busy = false;

  /// Log of the DHT measurement (R-012). It lives in a file on the Rust side, so it
  /// survives leaving this screen and stopping the app; this is only its copy.
  String _probe = '';
  bool _probing = false;

  @override
  void initState() {
    super.initState();
    _run();
    _showProbeLog();
  }

  Future<String> _readProbeLog() async {
    try {
      return await dhtProbeLog();
    } catch (_) {
      return '';
    }
  }

  Future<void> _showProbeLog() async {
    final log = await _readProbeLog();
    if (mounted) setState(() => _probe = log);
  }

  Future<void> _run() async {
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final checks = await selfCheck();
      final diagnostics = await platformDiagnostics();
      if (mounted) {
        setState(() {
          _checks = checks;
          _diagnostics = diagnostics;
        });
      }
    } catch (e) {
      if (mounted) setState(() => _error = e.toString());
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  /// Today in the desktop probe's format: its public items are named by day.
  static String _today() {
    final d = DateTime.now();
    String two(int v) => v.toString().padLeft(2, '0');
    return '${d.year}-${two(d.month)}-${two(d.day)}';
  }

  /// Runs one step of the DHT measurement. The step writes its report into the log
  /// itself; the screen then shows the log.
  Future<void> _measure(Future<String> Function() step) async {
    setState(() => _probing = true);
    var extra = '';
    try {
      final report = await step();
      // A report the log does not end with was not saved: show it anyway.
      extra = report;
    } catch (e) {
      extra = 'DHT probe failed: $e\n';
    }
    final log = await _readProbeLog();
    if (mounted) {
      setState(() {
        _probe = log.endsWith(extra) ? log : '$log$extra';
        _probing = false;
      });
    }
  }

  Future<void> _clearProbeLog() async {
    setState(() => _probing = true);
    var problem = '';
    try {
      problem = await dhtProbeClearLog();
    } catch (e) {
      problem = 'could not clear the log: $e';
    }
    final log = await _readProbeLog();
    if (mounted) {
      setState(() {
        _probe = problem.isEmpty ? log : '$log$problem\n';
        _probing = false;
      });
    }
  }

  /// Everything at once, as text — to send in a single message.
  String _asText() {
    final buffer = StringBuffer('Apeiron — самопроверка\n\n');
    for (final c in _checks ?? const <CheckLine>[]) {
      buffer.writeln('${c.passed ? "[ да ]" : "[ НЕТ]"}  ${c.name}');
      buffer.writeln('        ${c.detail}');
    }
    buffer.writeln();
    buffer.writeln(_diagnostics ?? '');
    if (_probe.isNotEmpty) {
      buffer.writeln();
      buffer.write(_probe);
    }
    return buffer.toString();
  }

  @override
  Widget build(BuildContext context) {
    final t = Theme.of(context).textTheme;
    final checks = _checks;
    final failed = checks?.where((c) => !c.passed).length ?? 0;

    return Scaffold(
      appBar: AppBar(
        title: const Text('САМОПРОВЕРКА'),
        actions: [
          IconButton(
            tooltip: 'Скопировать отчёт целиком',
            onPressed: checks == null
                ? null
                : () async {
                    await Clipboard.setData(ClipboardData(text: _asText()));
                    if (context.mounted) {
                      ScaffoldMessenger.of(context).showSnackBar(
                        const SnackBar(content: Text('Отчёт скопирован')),
                      );
                    }
                  },
            icon: const Icon(Icons.copy_all_outlined, color: Ap.fog400),
          ),
          IconButton(
            tooltip: 'Прогнать заново',
            onPressed: _busy ? null : _run,
            icon: const Icon(Icons.refresh, color: Ap.fog400),
          ),
          const SizedBox(width: Ap.s8),
        ],
      ),
      body: SafeArea(
        child: Center(
          child: ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 560),
            child: SingleChildScrollView(
              padding: const EdgeInsets.all(Ap.s20),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  if (_error != null) ...[
                    Text(
                      _error!,
                      style: t.bodySmall?.copyWith(color: Ap.rust500),
                    ),
                    const SizedBox(height: Ap.s20),
                  ],
                  if (checks != null) ...[
                    Text(
                      failed == 0
                          ? 'Всё прошло.'
                          : 'Не прошло: $failed из ${checks.length}.',
                      style: t.labelLarge?.copyWith(
                        color: failed == 0 ? Ap.glacier400 : Ap.ember400,
                      ),
                    ),
                    const SizedBox(height: Ap.s16),
                    for (final c in checks) _CheckRow(line: c),
                  ],
                  const SizedBox(height: Ap.s28),
                  Text('ЗАМЕР DHT', style: t.labelLarge),
                  const SizedBox(height: Ap.s8),
                  Text(
                    'Можно ли доставлять сообщения без единого сервера. '
                    'Конверты тестовые: случайные байты, ничего о вас. '
                    'Положите, через несколько часов проверьте свои, и '
                    'заберите конверты, которые положил ПК. Журнал '
                    'сохраняется между запусками.',
                    style: t.bodySmall,
                  ),
                  const SizedBox(height: Ap.s12),
                  Wrap(
                    spacing: Ap.s8,
                    runSpacing: Ap.s8,
                    children: [
                      OutlinedButton(
                        onPressed: _probing
                            ? null
                            : () => _measure(() => dhtProbePut(count: 12)),
                        child: const Text('ПОЛОЖИТЬ 12'),
                      ),
                      OutlinedButton(
                        onPressed: _probing
                            ? null
                            : () => _measure(dhtProbeGetOwn),
                        child: const Text('ПРОВЕРИТЬ СВОИ'),
                      ),
                      OutlinedButton(
                        onPressed: _probing
                            ? null
                            // Two sets from the desktop: one put once in the
                            // morning (does it survive?) and one re-put every
                            // half hour (can this phone fetch at all?). One
                            // call: one bootstrap, every lookup at once.
                            : () => _measure(
                                () => dhtProbeGetPublic(
                                  days: [_today(), '${_today()}/fresh'],
                                  count: 24,
                                ),
                              ),
                        child: const Text('ЗАБРАТЬ С ПК'),
                      ),
                      TextButton(
                        onPressed: _probing || _probe.isEmpty
                            ? null
                            : _clearProbeLog,
                        child: const Text('ОЧИСТИТЬ ЖУРНАЛ'),
                      ),
                    ],
                  ),
                  if (_probing) ...[
                    const SizedBox(height: Ap.s12),
                    const LinearProgressIndicator(),
                  ],
                  if (_probe.isNotEmpty) ...[
                    const SizedBox(height: Ap.s12),
                    Container(
                      width: double.infinity,
                      padding: const EdgeInsets.all(Ap.s12),
                      color: Ap.basalt800,
                      child: Text(
                        _probe,
                        style: Ap.mono(size: 12, color: Ap.fog400),
                      ),
                    ),
                  ],
                  if (_diagnostics != null) ...[
                    const SizedBox(height: Ap.s28),
                    Text('ПЛАТФОРМА', style: t.labelLarge),
                    const SizedBox(height: Ap.s8),
                    Text(
                      'Секретов здесь нет. Это то, что можно переслать целиком.',
                      style: t.bodySmall,
                    ),
                    const SizedBox(height: Ap.s12),
                    Container(
                      width: double.infinity,
                      padding: const EdgeInsets.all(Ap.s12),
                      color: Ap.basalt800,
                      child: Text(
                        _diagnostics!,
                        style: Ap.mono(size: 12, color: Ap.fog400),
                      ),
                    ),
                  ],
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}

class _CheckRow extends StatelessWidget {
  const _CheckRow({required this.line});

  final CheckLine line;

  @override
  Widget build(BuildContext context) {
    final t = Theme.of(context).textTheme;
    final color = line.passed ? Ap.glacier400 : Ap.ember400;
    return Padding(
      padding: const EdgeInsets.only(bottom: Ap.s12),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          SizedBox(
            width: 52,
            child: Text(
              line.passed ? '[ да ]' : '[ НЕТ]',
              style: Ap.mono(size: 12, color: color),
            ),
          ),
          const SizedBox(width: Ap.s8),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(line.name, style: t.bodyMedium?.copyWith(color: color)),
                const SizedBox(height: Ap.s4),
                Text(line.detail, style: t.bodySmall),
              ],
            ),
          ),
        ],
      ),
    );
  }
}
