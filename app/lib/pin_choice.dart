import 'package:flutter/material.dart';

import 'l10n/app_localizations.dart';
import 'theme/tokens.dart';

/// The lengths a new PIN may have; the same list as `PIN_LENGTHS` in `store/src/wrapper.rs`.
const pinLengths = [4, 6, 8];

/// What the interface recommends: the longest PIN and the scrambled keyboard.
const recommendedDigits = 8;

/// The choice made before a PIN is set: its length and the keyboard, with the recommendation
/// and what each length costs, said plainly (R-011: 4 digits fall in minutes to root).
class PinChoice extends StatefulWidget {
  const PinChoice({
    super.key,
    required this.onChosen,
    this.initialScrambled = true,
  });

  final void Function(int digits, bool scrambled) onChosen;
  final bool initialScrambled;

  @override
  State<PinChoice> createState() => _PinChoiceState();
}

class _PinChoiceState extends State<PinChoice> {
  int _digits = recommendedDigits;
  late bool _scrambled = widget.initialScrambled;

  @override
  Widget build(BuildContext context) {
    final t = Theme.of(context).textTheme;
    final l = AppLocalizations.of(context);

    Widget chip({
      required String label,
      required bool selected,
      required bool recommended,
      required VoidCallback onTap,
    }) => ChoiceChip(
      label: Text(label),
      selected: selected,
      onSelected: (_) => onTap(),
      avatar: recommended
          ? const Icon(Icons.star, size: 14, color: Ap.ember400)
          : null,
      showCheckmark: false,
    );

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Text(l.pinChoiceTitle, style: t.labelLarge),
        const SizedBox(height: Ap.s16),
        Text(l.pinChoiceLength, style: t.labelMedium),
        const SizedBox(height: Ap.s8),
        Wrap(
          spacing: Ap.s8,
          children: [
            for (final n in pinLengths)
              chip(
                label: l.pinDigits(n),
                selected: _digits == n,
                recommended: n == recommendedDigits,
                onTap: () => setState(() => _digits = n),
              ),
          ],
        ),
        const SizedBox(height: Ap.s16),
        Text(l.pinChoiceKeyboard, style: t.labelMedium),
        const SizedBox(height: Ap.s8),
        Wrap(
          spacing: Ap.s8,
          children: [
            chip(
              label: l.pinKeyboardScrambled,
              selected: _scrambled,
              recommended: true,
              onTap: () => setState(() => _scrambled = true),
            ),
            chip(
              label: l.pinKeyboardOrdered,
              selected: !_scrambled,
              recommended: false,
              onTap: () => setState(() => _scrambled = false),
            ),
          ],
        ),
        const SizedBox(height: Ap.s20),
        Text(
          l.pinChoiceRecommend,
          style: t.bodySmall?.copyWith(color: Ap.bone100),
        ),
        const SizedBox(height: Ap.s8),
        Text(l.pinChoiceHonest, style: t.bodySmall),
        const SizedBox(height: Ap.s20),
        FilledButton(
          onPressed: () => widget.onChosen(_digits, _scrambled),
          child: Text(l.pinChoiceContinue),
        ),
      ],
    );
  }
}
