import 'dart:async';

import 'package:flutter/foundation.dart' show defaultTargetPlatform;
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'check_screen.dart';
import 'fingerprint.dart';
import 'lock_policy.dart';
import 'raven.dart';
import 'pin_screen.dart';
import 'vault_panel.dart';
import 'vault_status.dart';
import 'wordmark.dart';
import 'src/rust/api/identity.dart';
import 'src/rust/api/vault.dart';
import 'src/rust/frb_generated.dart';
import 'theme/tokens.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  await RustLib.init();
  runApp(const ApeironApp());
}

class ApeironApp extends StatelessWidget {
  const ApeironApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'Apeiron',
      debugShowCheckedModeBanner: false,
      theme: Ap.dark(),
      home: const IdentityScreen(),
    );
  }
}

/// Identity screen — which is also the check of the stage 1 criterion.
///
/// Shows that Flutter reached Rust, Rust generated an Ed25519 + X25519 pair,
/// and only the public part came back out: the secret keys stayed in Rust
/// (R-004).
class IdentityScreen extends StatefulWidget {
  const IdentityScreen({super.key});

  @override
  State<IdentityScreen> createState() => _IdentityScreenState();
}

class _IdentityScreenState extends State<IdentityScreen>
    with WidgetsBindingObserver {
  PublicIdentityView? _identity;

  /// State of the key vault. `null` — not asked yet.
  VaultStatus? _vault;
  String? _error;
  bool _busy = false;

  /// Locking rules differ between the phone and the desktop — see [LockPolicy].
  final LockPolicy _policy = LockPolicy.of(defaultTargetPlatform);
  Timer? _idle;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    HardwareKeyboard.instance.addHandler(_onKey);
    _refresh();
  }

  @override
  void dispose() {
    _idle?.cancel();
    HardwareKeyboard.instance.removeHandler(_onKey);
    WidgetsBinding.instance.removeObserver(this);
    super.dispose();
  }

  /// Decision R-001: the identity locks by itself. Rust destroys the keys and
  /// overwrites them — they were never in Dart. What counts as a trigger is
  /// decided by [LockPolicy]: on the phone it is leaving the foreground, on the
  /// desktop a minimised window or inactivity, but not switching to another
  /// window.
  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (_policy.locksOn(state)) _lock();
  }

  bool _onKey(KeyEvent event) {
    _noteActivity();
    return false; // not our event, pass it down the chain
  }

  /// Any user action pushes back the idle timer.
  void _noteActivity() {
    final timeout = _policy.idleTimeout;
    if (timeout == null || _identity == null) return;
    _idle?.cancel();
    _idle = Timer(timeout, _lock);
  }

  Future<void> _run(Future<void> Function() action) async {
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      await action();
    } catch (e) {
      if (mounted) setState(() => _error = e.toString());
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  /// Asks where the vault stands and, if it is open, reads the identity.
  ///
  /// Nothing is opened here: opening needs the PIN (R-001), and the PIN screen
  /// takes over when the vault is locked or has no PIN yet.
  Future<void> _refresh() => _run(() async {
    await _show(await vaultStatus());
  });

  /// Shows a new vault state; reads the identity if the vault is open.
  Future<void> _show(VaultStatus vault) async {
    final id = vault.state == VaultState.opened
        ? await currentIdentity()
        : null;
    if (mounted) {
      setState(() {
        _vault = vault;
        _identity = id;
      });
    }
    _noteActivity();
  }

  /// Clears data of a test build before the PIN, at the owner's request.
  Future<void> _resetLegacy() => _run(() async {
    await _show(await resetLegacy());
  });

  /// Erases everything cryptographically (R-005): the key is destroyed, not
  /// the data.
  ///
  /// Offered in exactly one state — when the key is really gone and there is
  /// nothing to decrypt with. In all others the data is intact, and erasing it
  /// on the owner's behalf is not allowed.
  Future<void> _freshStart() => _run(() async {
    await wipeEverything();
    if (mounted) {
      setState(() {
        _identity = null;
        _vault = null;
      });
    }
    await _refresh();
  });

  Future<void> _generate() => _run(() async {
    final id = await generateIdentity();
    if (mounted) setState(() => _identity = id);
    _noteActivity();
  });

  Future<void> _lock() => _run(() async {
    _idle?.cancel();
    _idle = null;
    await lockIdentity();
    final vault = await vaultStatus();
    if (mounted) {
      setState(() {
        _identity = null;
        _vault = vault;
      });
    }
  });

  @override
  Widget build(BuildContext context) {
    final id = _identity;
    final vault = _vault;
    return Listener(
      // Push back the idle timer. `translucent` so that events also reach
      // the widgets beneath us: we listen, we do not intercept.
      behavior: HitTestBehavior.translucent,
      onPointerDown: (_) => _noteActivity(),
      onPointerMove: (_) => _noteActivity(),
      onPointerHover: (_) => _noteActivity(),
      onPointerSignal: (_) => _noteActivity(),
      child: Scaffold(
        appBar: AppBar(
          titleSpacing: Ap.s20,
          title: Row(
            children: [
              const ApeironRaven(size: 26),
              const SizedBox(width: Ap.s12),
              const ApeironWordmark(height: 19),
            ],
          ),
          actions: [
            if (id != null)
              IconButton(
                tooltip: 'Самопроверка',
                onPressed: _busy
                    ? null
                    : () => Navigator.of(context).push(
                        MaterialPageRoute<void>(
                          builder: (_) => const CheckScreen(),
                        ),
                      ),
                icon: const Icon(Icons.fact_check_outlined, color: Ap.fog400),
              ),
            if (id != null)
              IconButton(
                tooltip: 'Заблокировать',
                onPressed: _busy ? null : _lock,
                icon: const Icon(Icons.lock_outline, color: Ap.fog400),
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
                    if (_error != null) _ErrorBanner(message: _error!),
                    if (vault != null &&
                        (needsPin(vault) || needsPinSetup(vault)))
                      PinScreen(
                        // Setting and entering are different screens; a wrong
                        // PIN or a delay is handled inside, without a rebuild.
                        key: ValueKey(needsPinSetup(vault)),
                        status: vault,
                        onDone: _show,
                      )
                    else if (vault != null) ...[
                      VaultPanel(
                        status: vault,
                        onRetry: vault.state == VaultState.retry && !_busy
                            ? _refresh
                            : null,
                        onFreshStart: _busy ? null : _freshStart,
                        onResetLegacy: _busy ? null : _resetLegacy,
                      ),
                      const SizedBox(height: Ap.s20),
                    ],
                    if (vault != null && vault.state == VaultState.opened)
                      if (id == null)
                        _LockedState(busy: _busy, onGenerate: _generate)
                      else
                        _IdentityView(identity: id),
                    const SizedBox(height: Ap.s40),
                    _HonestNote(policy: _policy),
                  ],
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}

class _LockedState extends StatelessWidget {
  const _LockedState({required this.busy, required this.onGenerate});

  final bool busy;
  final VoidCallback onGenerate;

  @override
  Widget build(BuildContext context) {
    final t = Theme.of(context).textTheme;
    return Column(
      children: [
        const SizedBox(height: Ap.s40),
        const ApeironRaven(size: 92, color: Ap.stone600),
        const SizedBox(height: Ap.s28),
        Text(
          'ЛИЧНОСТИ ЕЩЁ НЕТ',
          style: t.labelLarge?.copyWith(color: Ap.bone100),
          textAlign: TextAlign.center,
        ),
        const SizedBox(height: Ap.s12),
        Text(
          'В хранилище этого устройства личности ещё нет. Вместе с ней будут '
          'заведены ключи устройства и журнал личности — порознь они '
          'бессмысленны.',
          style: t.bodySmall,
          textAlign: TextAlign.center,
        ),
        const SizedBox(height: Ap.s28),
        FilledButton(
          onPressed: busy ? null : onGenerate,
          child: const Text('СОЗДАТЬ ЛИЧНОСТЬ'),
        ),
      ],
    );
  }
}

class _IdentityView extends StatelessWidget {
  const _IdentityView({required this.identity});

  final PublicIdentityView identity;

  @override
  Widget build(BuildContext context) {
    final t = Theme.of(context).textTheme;
    final groups = identity.fingerprint.split(' ');

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Text('ОТПЕЧАТОК', style: t.labelLarge),
        const SizedBox(height: Ap.s8),
        Text(
          'Это читают вслух собеседнику. Расхождение хотя бы в одной цифре '
          'означает, что между вами кто-то есть.',
          style: t.bodySmall,
        ),
        const SizedBox(height: Ap.s16),

        // The copper accent is used only here. Key verification looks like
        // nothing else in the app, on purpose: people must look at it.
        Container(
          decoration: const BoxDecoration(
            color: Ap.basalt800,
            border: Border(
              left: BorderSide(color: Ap.ember400, width: 3),
              top: BorderSide(color: Ap.stone700),
              right: BorderSide(color: Ap.stone700),
              bottom: BorderSide(color: Ap.stone700),
            ),
          ),
          padding: const EdgeInsets.symmetric(
            vertical: Ap.s28,
            horizontal: Ap.s16,
          ),
          // A rigid 3 × 2 grid, not Wrap: the split must be the same on
          // every screen. When verifying by voice, a shifting layout is a
          // source of errors, and an error here means a missed man in the
          // middle.
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              for (final (i, row) in fingerprintRows(groups, 3).indexed) ...[
                if (i > 0) const SizedBox(height: Ap.s16),
                Row(
                  mainAxisAlignment: MainAxisAlignment.spaceEvenly,
                  children: [
                    for (final g in row)
                      Flexible(
                        child: FittedBox(
                          fit: BoxFit.scaleDown,
                          child: Text(
                            g,
                            style: Ap.mono(
                              size: 26,
                              spacing: 3.4,
                              weight: FontWeight.w600,
                            ),
                          ),
                        ),
                      ),
                  ],
                ),
              ],
            ],
          ),
        ),
        const SizedBox(height: Ap.s12),
        Row(
          children: [
            Container(width: 7, height: 7, color: Ap.ember400),
            const SizedBox(width: Ap.s8),
            Text(
              'НЕ СВЕРЕНО НИ С КЕМ',
              style: t.labelMedium?.copyWith(color: Ap.ember400),
            ),
          ],
        ),

        const SizedBox(height: Ap.s28),
        _KeyRow(label: 'ED25519 · ПОДПИСЬ', value: identity.signingKeyHex),
        const SizedBox(height: Ap.s16),
        _KeyRow(
          label: 'X25519 · СОГЛАСОВАНИЕ',
          value: identity.agreementKeyHex,
        ),
      ],
    );
  }
}

class _KeyRow extends StatelessWidget {
  const _KeyRow({required this.label, required this.value});

  final String label;
  final String value;

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          children: [
            Expanded(
              child: Text(
                label,
                style: Theme.of(context).textTheme.labelMedium,
              ),
            ),
            IconButton(
              iconSize: 16,
              visualDensity: VisualDensity.compact,
              tooltip: 'Скопировать',
              onPressed: () {
                Clipboard.setData(ClipboardData(text: value));
                ScaffoldMessenger.of(
                  context,
                ).showSnackBar(const SnackBar(content: Text('Скопировано')));
              },
              icon: const Icon(Icons.content_copy, color: Ap.fog400),
            ),
          ],
        ),
        const SizedBox(height: Ap.s4),
        SelectableText(value, style: Ap.mono(size: 12, color: Ap.fog400)),
      ],
    );
  }
}

class _HonestNote extends StatelessWidget {
  const _HonestNote({required this.policy});

  final LockPolicy policy;

  @override
  Widget build(BuildContext context) {
    final t = Theme.of(context).textTheme;
    return Container(
      padding: const EdgeInsets.all(Ap.s16),
      decoration: BoxDecoration(border: Border.all(color: Ap.stone700)),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text('ЧТО ЗДЕСЬ УЖЕ ПРАВДА', style: t.labelLarge),
          const SizedBox(height: Ap.s8),
          Text(
            'Секретные ключи не покидают Rust — сюда пришли только публичные '
            'половины и отпечаток. ${policy.explanation}',
            style: t.bodySmall,
          ),
          const SizedBox(height: Ap.s16),
          Text('ЧЕГО ЕЩЁ НЕТ', style: t.labelLarge),
          const SizedBox(height: Ap.s8),
          Text(
            'Переписки пока нет. В сеть приложение ходит только ради замера '
            'DHT на экране самопроверки: тестовые конверты без содержимого.',
            style: t.bodySmall,
          ),
        ],
      ),
    );
  }
}

class _ErrorBanner extends StatelessWidget {
  const _ErrorBanner({required this.message});

  final String message;

  @override
  Widget build(BuildContext context) {
    return Container(
      margin: const EdgeInsets.only(bottom: Ap.s20),
      padding: const EdgeInsets.all(Ap.s12),
      decoration: const BoxDecoration(
        color: Ap.basalt800,
        border: Border(left: BorderSide(color: Ap.rust500, width: 3)),
      ),
      child: Text(message, style: Ap.mono(size: 12, color: Ap.rust500)),
    );
  }
}
