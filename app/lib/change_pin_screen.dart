import 'package:flutter/material.dart';

import 'l10n/app_localizations.dart';
import 'pin_choice.dart';
import 'pin_pad.dart';
import 'src/rust/api/pin.dart';
import 'src/rust/api/vault.dart';
import 'theme/tokens.dart';
import 'vault_status.dart';

enum _Step { current, choose, fresh, repeat }

/// Changing the PIN: the current one, then the new length and keyboard, then the new PIN
/// twice. Rust keeps every entry; the current PIN is tried once, at the end, together with
/// the new one — a wrong one is counted like any wrong PIN. Nothing is re-encrypted.
class ChangePinScreen extends StatefulWidget {
  const ChangePinScreen({super.key});

  @override
  State<ChangePinScreen> createState() => _ChangePinScreenState();
}

class _ChangePinScreenState extends State<ChangePinScreen> {
  _Step _step = _Step.current;
  PinPadPrefs? _prefs;
  int _newDigits = recommendedDigits;
  List<int> _layout = const [];
  int _entered = 0;
  bool _busy = false;
  String? _message;

  int get _digits => switch (_step) {
    _Step.current => _prefs?.digits ?? 0,
    _ => _newDigits,
  };

  @override
  void initState() {
    super.initState();
    _start();
  }

  @override
  void dispose() {
    pinPadClear();
    super.dispose();
  }

  Future<void> _start() async {
    final prefs = await pinPadPrefs();
    if (!mounted) return;
    setState(() {
      _prefs = prefs;
      _step = _Step.current;
    });
    await _begin();
  }

  Future<void> _begin() async {
    final layout = await pinPadBegin();
    if (mounted) {
      setState(() {
        _layout = layout.toList();
        _entered = 0;
      });
    }
  }

  Future<void> _press(int position) async {
    final n = await pinPadPress(position: position);
    if (!mounted) return;
    setState(() => _entered = n);
    if (_digits > 0 && n == _digits && !_busy) await _submit();
  }

  Future<void> _erase() async {
    final n = await pinPadErase();
    if (mounted) setState(() => _entered = n);
  }

  Future<void> _chosen(int digits, bool scrambled) async {
    // The keyboard applies at once; the length only once the new PIN is set.
    await setPinPadPrefs(digits: _prefs?.digits ?? 0, scrambled: scrambled);
    if (!mounted) return;
    setState(() {
      _newDigits = digits;
      _prefs = PinPadPrefs(digits: _prefs?.digits ?? 0, scrambled: scrambled);
      _step = _Step.fresh;
    });
    await _begin();
  }

  Future<void> _submit() async {
    final l = AppLocalizations.of(context);
    setState(() {
      _busy = true;
      _message = null;
    });
    try {
      switch (_step) {
        case _Step.current:
          await pinChangeCurrent();
          setState(() => _step = _Step.choose);
        case _Step.choose:
          break;
        case _Step.fresh:
          await pinSetupFirst();
          setState(() => _step = _Step.repeat);
          await _begin();
        case _Step.repeat:
          final result = await pinChangeConfirm();
          if (!mounted) return;
          if (result.state == VaultState.opened) {
            ScaffoldMessenger.of(
              context,
            ).showSnackBar(SnackBar(content: Text(l.pinChanged)));
            Navigator.of(context).pop();
            return;
          }
          // Wrong, delayed or mismatched: from the beginning, with the reason.
          final said = describeVault(result, l);
          setState(() => _message = '${said.title}. ${said.detail}');
          await _start();
      }
    } catch (e) {
      if (mounted) setState(() => _message = e.toString());
      await _start();
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final t = Theme.of(context).textTheme;
    final l = AppLocalizations.of(context);
    final heading = switch (_step) {
      _Step.current => l.pinCurrent,
      _Step.choose => '',
      _Step.fresh => l.pinNew,
      _Step.repeat => l.pinRepeat,
    };
    return Scaffold(
      appBar: AppBar(title: Text(l.changePinTitle, style: t.labelLarge)),
      body: SafeArea(
        child: Center(
          child: ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 560),
            child: SingleChildScrollView(
              padding: const EdgeInsets.all(Ap.s20),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  if (_message != null) ...[
                    Text(
                      _message!,
                      style: t.bodySmall?.copyWith(color: Ap.rust500),
                    ),
                    const SizedBox(height: Ap.s20),
                  ],
                  if (_step == _Step.choose)
                    PinChoice(
                      onChosen: _chosen,
                      initialScrambled: _prefs?.scrambled ?? true,
                    )
                  else ...[
                    Text(
                      heading,
                      style: t.labelMedium,
                      textAlign: TextAlign.center,
                    ),
                    const SizedBox(height: Ap.s16),
                    PinPad(
                      layout: _layout,
                      entered: _entered,
                      busy: _busy,
                      minLength: _digits > 0 ? _digits : 4,
                      maxLength: _digits > 0 ? _digits : 16,
                      onPress: _press,
                      onErase: _erase,
                      onSubmit: _submit,
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
