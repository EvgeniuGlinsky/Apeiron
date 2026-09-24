import 'dart:async';

import 'package:flutter/material.dart';

import 'l10n/app_localizations.dart';
import 'pin_pad.dart';
import 'src/rust/api/pin.dart';
import 'src/rust/api/vault.dart';
import 'theme/tokens.dart';
import 'vault_status.dart';

/// Setting the PIN and entering it (R-001, R-007, R-011).
///
/// The screen holds no digits. Every attempt starts with a fresh layout drawn
/// by Rust; taps go to Rust as positions; Rust compares the two entries of a
/// new PIN and runs the hardware chain. What comes back is a [VaultStatus]:
/// opened, wrong, or wait.
class PinScreen extends StatefulWidget {
  const PinScreen({super.key, required this.status, required this.onDone});

  /// The state that brought the person here.
  final VaultStatus status;

  /// Called with the status once the vault is open, or once the state is one
  /// this screen does not handle (key gone, legacy data and so on).
  final ValueChanged<VaultStatus> onDone;

  @override
  State<PinScreen> createState() => _PinScreenState();
}

class _PinScreenState extends State<PinScreen> {
  List<int> _layout = const [];
  int _entered = 0;
  bool _busy = false;

  /// While a new PIN is being set: whether this is the second entry.
  bool _confirming = false;

  late VaultStatus _status = widget.status;
  String? _error;

  int _waitLeft = 0;
  Timer? _tick;

  bool get _setup => needsPinSetup(_status);

  @override
  void initState() {
    super.initState();
    _startWait(widget.status.waitSeconds);
    _begin();
  }

  @override
  void dispose() {
    _tick?.cancel();
    // Whatever was typed and not submitted must not outlive the screen.
    pinPadClear();
    super.dispose();
  }

  Future<void> _begin() async {
    try {
      final layout = await pinPadBegin();
      if (mounted) {
        setState(() {
          _layout = layout.toList();
          _entered = 0;
        });
      }
    } catch (e) {
      if (mounted) setState(() => _error = e.toString());
    }
  }

  void _startWait(int seconds) {
    _tick?.cancel();
    _waitLeft = seconds;
    if (seconds <= 0) return;
    _tick = Timer.periodic(const Duration(seconds: 1), (t) {
      if (!mounted) return t.cancel();
      setState(() => _waitLeft = _waitLeft > 0 ? _waitLeft - 1 : 0);
      if (_waitLeft == 0) t.cancel();
    });
  }

  Future<void> _press(int position) async {
    final n = await pinPadPress(position: position);
    if (mounted) setState(() => _entered = n);
  }

  Future<void> _erase() async {
    final n = await pinPadErase();
    if (mounted) setState(() => _entered = n);
  }

  Future<void> _submit() async {
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      if (_setup && !_confirming) {
        await pinSetupFirst();
        _confirming = true;
        await _begin();
        return;
      }
      final result = _setup ? await pinSetupConfirm() : await unlockWithPin();
      if (!mounted) return;
      switch (result.state) {
        case VaultState.pinMismatch:
          _confirming = false;
          setState(() => _status = result);
          await _begin();
        case VaultState.wrongPin:
        case VaultState.delayed:
        case VaultState.retry:
          setState(() => _status = result);
          _startWait(result.waitSeconds);
          await _begin();
        default:
          widget.onDone(result);
      }
    } catch (e) {
      // pinSetupFirst refuses a PIN of impossible length; the pad starts over.
      _confirming = false;
      if (mounted) setState(() => _error = e.toString());
      await _begin();
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final t = Theme.of(context).textTheme;
    final l = AppLocalizations.of(context);
    final message = describeVault(_status, l);
    final waiting = _waitLeft > 0;
    final heading = _setup
        ? (_confirming ? l.pinRepeat : l.pinNew)
        : l.pinEnter;

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Text(message.title.toUpperCase(), style: t.labelLarge),
        const SizedBox(height: Ap.s8),
        Text(message.detail, style: t.bodySmall),
        if (!_confirming && message.action.isNotEmpty) ...[
          const SizedBox(height: Ap.s8),
          Text(
            waiting ? l.nextAttemptIn(waitLabel(_waitLeft, l)) : message.action,
            style: t.bodySmall?.copyWith(color: Ap.bone100),
          ),
        ],
        if (_error != null) ...[
          const SizedBox(height: Ap.s8),
          Text(_error!, style: t.bodySmall?.copyWith(color: Ap.rust500)),
        ],
        const SizedBox(height: Ap.s28),
        Text(heading, style: t.labelMedium, textAlign: TextAlign.center),
        const SizedBox(height: Ap.s16),
        PinPad(
          layout: _layout,
          entered: _entered,
          busy: _busy || waiting,
          onPress: _press,
          onErase: _erase,
          onSubmit: _submit,
        ),
        const SizedBox(height: Ap.s16),
        Text(l.pinLayoutNote, style: t.bodySmall, textAlign: TextAlign.center),
      ],
    );
  }
}
