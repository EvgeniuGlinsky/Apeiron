import 'dart:async';

import 'package:flutter/foundation.dart' show defaultTargetPlatform;
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'chats_screen.dart';
import 'identity_view.dart';
import 'l10n/app_localizations.dart';
import 'locale_choice.dart';
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
      localizationsDelegates: AppLocalizations.localizationsDelegates,
      supportedLocales: AppLocalizations.supportedLocales,
      localeListResolutionCallback: chooseLocale,
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
    // Every screen opened over this one shows what the keys opened: all go.
    if (mounted) Navigator.of(context).popUntil((route) => route.isFirst);
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
    if (id != null && vault != null && vault.state == VaultState.opened) {
      return Listener(
        behavior: HitTestBehavior.translucent,
        onPointerDown: (_) => _noteActivity(),
        onPointerMove: (_) => _noteActivity(),
        onPointerHover: (_) => _noteActivity(),
        onPointerSignal: (_) => _noteActivity(),
        child: ChatsHome(identity: id, policy: _policy, onLock: _lock),
      );
    }
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
          actions: [const SizedBox(width: Ap.s8)],
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
                    if (vault != null &&
                        vault.state == VaultState.opened &&
                        id == null)
                      _LockedState(busy: _busy, onGenerate: _generate),
                    const SizedBox(height: Ap.s40),
                    HonestNote(policy: _policy),
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
    final l = AppLocalizations.of(context);
    return Column(
      children: [
        const SizedBox(height: Ap.s40),
        const ApeironRaven(size: 92, color: Ap.stone600),
        const SizedBox(height: Ap.s28),
        Text(
          l.noIdentityTitle,
          style: t.labelLarge?.copyWith(color: Ap.bone100),
          textAlign: TextAlign.center,
        ),
        const SizedBox(height: Ap.s12),
        Text(l.noIdentityBody, style: t.bodySmall, textAlign: TextAlign.center),
        const SizedBox(height: Ap.s28),
        FilledButton(
          onPressed: busy ? null : onGenerate,
          child: Text(l.createIdentity),
        ),
      ],
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
