import 'package:flutter/material.dart';

import 'l10n/app_localizations.dart';
import 'theme/tokens.dart';

/// Scrambled PIN pad (R-007).
///
/// The widget never learns the PIN. The layout comes from Rust, which draws a
/// new permutation for every attempt; a tap sends only the **position** of the
/// key, and Rust maps it to a digit and keeps the digits itself. Dart holds the
/// layout and the number of dots — nothing that is the PIN as a string.
///
/// That narrows what Dart could leak; it does not make the screen secret. The
/// digits are drawn on screen, so a camera that sees the screen sees the PIN.
/// The scrambling protects against someone who sees only the finger.
class PinPad extends StatelessWidget {
  const PinPad({
    super.key,
    required this.layout,
    required this.entered,
    required this.onPress,
    required this.onErase,
    required this.onSubmit,
    this.minLength = 6,
    this.maxLength = 16,
    this.busy = false,
  });

  /// Digit shown at each of the ten positions, in reading order: three rows
  /// of three, then the middle of the bottom row.
  final List<int> layout;

  /// How many digits are entered so far.
  final int entered;

  final ValueChanged<int> onPress;
  final VoidCallback onErase;
  final VoidCallback onSubmit;
  final int minLength;
  final int maxLength;
  final bool busy;

  @override
  Widget build(BuildContext context) {
    final l = AppLocalizations.of(context);
    final canPress = !busy && entered < maxLength && layout.length == 10;
    final canSubmit = !busy && entered >= minLength;

    Widget digitKey(int position) {
      final digit = position < layout.length ? layout[position] : null;
      return _Key(
        onTap: canPress ? () => onPress(position) : null,
        child: Text(
          digit == null ? '' : '$digit',
          style: Ap.mono(size: 26, weight: FontWeight.w600),
        ),
      );
    }

    return Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        _Dots(entered: entered, minLength: minLength),
        const SizedBox(height: Ap.s28),
        for (var row = 0; row < 3; row++) ...[
          Row(
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              for (var col = 0; col < 3; col++) digitKey(row * 3 + col),
            ],
          ),
        ],
        Row(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            _Key(
              onTap: !busy && entered > 0 ? onErase : null,
              semanticLabel: l.pinErase,
              child: const Icon(Icons.backspace_outlined, color: Ap.fog400),
            ),
            digitKey(9),
            _Key(
              onTap: canSubmit ? onSubmit : null,
              semanticLabel: l.pinDone,
              child: busy
                  ? const SizedBox(
                      width: 20,
                      height: 20,
                      child: CircularProgressIndicator(strokeWidth: 2),
                    )
                  : Icon(
                      Icons.arrow_forward,
                      color: canSubmit ? Ap.bone100 : Ap.stone600,
                    ),
            ),
          ],
        ),
      ],
    );
  }
}

class _Key extends StatelessWidget {
  const _Key({required this.child, this.onTap, this.semanticLabel});

  final Widget child;
  final VoidCallback? onTap;
  final String? semanticLabel;

  @override
  Widget build(BuildContext context) {
    final key = Padding(
      padding: const EdgeInsets.all(Ap.s4),
      child: Material(
        color: Ap.basalt800,
        shape: const RoundedRectangleBorder(
          side: BorderSide(color: Ap.stone700),
        ),
        child: InkWell(
          onTap: onTap,
          child: SizedBox(width: 76, height: 64, child: Center(child: child)),
        ),
      ),
    );
    final label = semanticLabel;
    return label == null
        ? key
        : Semantics(label: label, button: true, child: key);
  }
}

/// Entered digits as dots. The minimum length is marked, so it is visible when
/// the PIN is long enough, without showing anything about the digits.
class _Dots extends StatelessWidget {
  const _Dots({required this.entered, required this.minLength});

  final int entered;
  final int minLength;

  @override
  Widget build(BuildContext context) {
    final slots = entered > minLength ? entered : minLength;
    return Wrap(
      spacing: Ap.s12,
      runSpacing: Ap.s8,
      alignment: WrapAlignment.center,
      children: [
        for (var i = 0; i < slots; i++)
          Container(
            width: 12,
            height: 12,
            decoration: BoxDecoration(
              color: i < entered ? Ap.bone100 : null,
              border: Border.all(color: i < entered ? Ap.bone100 : Ap.stone600),
            ),
          ),
      ],
    );
  }
}
