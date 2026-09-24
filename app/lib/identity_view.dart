import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'fingerprint.dart';
import 'l10n/app_localizations.dart';
import 'lock_policy.dart';
import 'src/rust/api/identity.dart';
import 'theme/tokens.dart';

/// Thirty digits in a rigid 3 × 2 grid, with the copper accent that nothing else in the app
/// uses: the fingerprint, and the safety number with a contact. People must look at it.
///
/// A rigid grid, not Wrap: the split must be the same on every screen. When verifying by
/// voice, a shifting layout is a source of errors, and an error here means a missed man in the
/// middle.
class DigitGrid extends StatelessWidget {
  const DigitGrid({super.key, required this.digits});

  /// Groups separated by spaces, as the core returns them.
  final String digits;

  @override
  Widget build(BuildContext context) {
    final groups = digits.split(' ');
    return Container(
      decoration: const BoxDecoration(
        color: Ap.basalt800,
        border: Border(
          left: BorderSide(color: Ap.ember400, width: 3),
          top: BorderSide(color: Ap.stone700),
          right: BorderSide(color: Ap.stone700),
          bottom: BorderSide(color: Ap.stone700),
        ),
      ),
      padding: const EdgeInsets.symmetric(vertical: Ap.s28, horizontal: Ap.s16),
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
    );
  }
}

/// The owner's fingerprint and public keys.
class IdentityView extends StatelessWidget {
  const IdentityView({super.key, required this.identity});

  final PublicIdentityView identity;

  @override
  Widget build(BuildContext context) {
    final t = Theme.of(context).textTheme;
    final l = AppLocalizations.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Text(l.fingerprintTitle, style: t.labelLarge),
        const SizedBox(height: Ap.s8),
        Text(l.fingerprintBody, style: t.bodySmall),
        const SizedBox(height: Ap.s16),
        DigitGrid(digits: identity.fingerprint),
        const SizedBox(height: Ap.s28),
        KeyRow(label: l.keySigning, value: identity.signingKeyHex),
        const SizedBox(height: Ap.s16),
        KeyRow(label: l.keyAgreement, value: identity.agreementKeyHex),
      ],
    );
  }
}

class KeyRow extends StatelessWidget {
  const KeyRow({super.key, required this.label, required this.value});

  final String label;
  final String value;

  @override
  Widget build(BuildContext context) {
    final l = AppLocalizations.of(context);
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
              tooltip: l.copy,
              onPressed: () {
                Clipboard.setData(ClipboardData(text: value));
                ScaffoldMessenger.of(
                  context,
                ).showSnackBar(SnackBar(content: Text(l.copied)));
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

/// What is true here and how messages travel, said plainly.
class HonestNote extends StatelessWidget {
  const HonestNote({super.key, required this.policy});

  final LockPolicy policy;

  @override
  Widget build(BuildContext context) {
    final t = Theme.of(context).textTheme;
    final l = AppLocalizations.of(context);
    return Container(
      padding: const EdgeInsets.all(Ap.s16),
      decoration: BoxDecoration(border: Border.all(color: Ap.stone700)),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(l.trueNowTitle, style: t.labelLarge),
          const SizedBox(height: Ap.s8),
          Text(l.trueNowBody(policy.explanation(l)), style: t.bodySmall),
          const SizedBox(height: Ap.s16),
          Text(l.notYetTitle, style: t.labelLarge),
          const SizedBox(height: Ap.s8),
          Text(l.notYetBody, style: t.bodySmall),
        ],
      ),
    );
  }
}
