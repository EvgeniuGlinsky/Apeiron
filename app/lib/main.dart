import 'dart:async';

import 'package:flutter/foundation.dart' show defaultTargetPlatform;
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'check_screen.dart';
import 'fingerprint.dart';
import 'lock_policy.dart';
import 'raven.dart';
import 'vault_panel.dart';
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

/// Экран личности — он же проверка критерия этапа 1.
///
/// Показывает, что Flutter дошёл до Rust, Rust породил пару Ed25519 + X25519,
/// а наружу вернулось только публичное: секретные ключи остались в Rust (R-004).
class IdentityScreen extends StatefulWidget {
  const IdentityScreen({super.key});

  @override
  State<IdentityScreen> createState() => _IdentityScreenState();
}

class _IdentityScreenState extends State<IdentityScreen>
    with WidgetsBindingObserver {
  PublicIdentityView? _identity;

  /// Положение хранилища ключа. `null` — ещё не спрашивали.
  VaultStatus? _vault;
  String? _error;
  bool _busy = false;

  /// Правила запирания у телефона и у рабочего стола разные — см. [LockPolicy].
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

  /// Решение R-001: личность запирается сама. Rust уничтожает ключи и затирает
  /// их — в Dart их и не было. Что считать поводом, решает [LockPolicy]:
  /// на телефоне это уход с переднего плана, на рабочем столе — свёрнутое окно
  /// или бездействие, но не переключение на другое окно.
  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (_policy.locksOn(state)) _lock();
  }

  bool _onKey(KeyEvent event) {
    _noteActivity();
    return false; // событие не наше, дальше по цепочке
  }

  /// Любое действие пользователя отодвигает таймер бездействия.
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

  /// Открывает хранилище и читает из него личность.
  ///
  /// Разблокировка идёт первой и всегда: ключ базы разворачивается аппаратным
  /// хранилищем устройства, и без него читать нечего. На запертом телефоне
  /// железо этого не сделает — так и задумано (R-001).
  Future<void> _refresh() => _run(() async {
    final vault = await unlockVault();
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
  });

  /// Стирает всё криптографически (R-005): уничтожается ключ, а не данные.
  ///
  /// Предлагается ровно в одном положении — когда ключ действительно исчез и
  /// расшифровать нечем. Во всех остальных данные целы, и стирать их за
  /// владельца нельзя.
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
      // Отодвигаем таймер бездействия. `translucent`, чтобы события доходили
      // и до виджетов под нами: мы слушаем, а не перехватываем.
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
                    if (vault != null) ...[
                      VaultPanel(
                        status: vault,
                        onRetry: vault.state == VaultState.opened || _busy
                            ? null
                            : _refresh,
                        onFreshStart: _busy ? null : _freshStart,
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

        // Медный акцент — только здесь. Сверка ключей не похожа ни на что
        // другое в приложении, и это намеренно: на неё должны смотреть.
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
          // Жёсткая сетка 3 × 2, а не Wrap: разбивка обязана быть одинаковой
          // на всех экранах. При сверке голосом плавающая раскладка —
          // источник ошибок, а ошибка здесь означает пропущенного посредника.
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
            'Ни переписки, ни сети, ни постоянного хранения. Это каркас: '
            'проверка того, что Flutter дошёл до Rust, а Rust породил ключи.',
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
