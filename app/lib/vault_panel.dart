import 'package:flutter/material.dart';

import 'src/rust/api/vault.dart';
import 'theme/tokens.dart';
import 'vault_status.dart';

/// State of the key vault — how it looks on screen.
///
/// Always shown, not only when something is wrong. R-002 requires an honest
/// warning where there is no hardware keystore; but the converse holds too:
/// a person must see what the protection rests on without waiting for it to
/// fail.
class VaultPanel extends StatelessWidget {
  const VaultPanel({
    super.key,
    required this.status,
    this.onRetry,
    this.onFreshStart,
    this.onResetLegacy,
  });

  final VaultStatus status;

  /// Retry. The data stays intact.
  final VoidCallback? onRetry;

  /// Start over. Offered only when the key is really gone.
  final VoidCallback? onFreshStart;

  /// Clear the data of a test build before the PIN. Offered only in that state.
  final VoidCallback? onResetLegacy;

  @override
  Widget build(BuildContext context) {
    final t = Theme.of(context).textTheme;
    final message = describeVault(status);
    final accent = _accent(message.tone);

    return Container(
      padding: const EdgeInsets.all(Ap.s16),
      decoration: BoxDecoration(
        color: Ap.basalt800,
        border: Border(left: BorderSide(color: accent, width: 2)),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            children: [
              Icon(_icon(message.tone), size: 16, color: accent),
              const SizedBox(width: Ap.s8),
              Expanded(
                child: Text(
                  message.title.toUpperCase(),
                  style: t.labelLarge?.copyWith(color: accent),
                ),
              ),
            ],
          ),
          const SizedBox(height: Ap.s12),
          Text(message.detail, style: t.bodySmall),
          if (message.action.isNotEmpty) ...[
            const SizedBox(height: Ap.s12),
            Text(
              message.action,
              style: t.bodySmall?.copyWith(color: Ap.bone100),
            ),
          ],
          if (onRetry != null ||
              (onFreshStart != null && mayOfferFreshStart(status)) ||
              (onResetLegacy != null && isLegacy(status))) ...[
            const SizedBox(height: Ap.s16),
            Row(
              children: [
                if (onRetry != null)
                  FilledButton(
                    onPressed: onRetry,
                    // The label must match what the text next to it offers:
                    // "retry" after "unlock" reads as two different actions,
                    // although it is one.
                    child: const Text('ПОВТОРИТЬ'),
                  ),
                // The "start over" button exists in exactly one state.
                // Offering to erase everything on a transient firmware failure
                // would mean destroying the owner's messages on their behalf.
                if (onFreshStart != null && mayOfferFreshStart(status)) ...[
                  const SizedBox(width: Ap.s12),
                  OutlinedButton(
                    onPressed: onFreshStart,
                    child: const Text('НАЧАТЬ ЗАНОВО'),
                  ),
                ],
                if (onResetLegacy != null && isLegacy(status))
                  OutlinedButton(
                    onPressed: onResetLegacy,
                    child: const Text('НАЧАТЬ ЗАНОВО С ПИНОМ'),
                  ),
              ],
            ),
          ],
        ],
      ),
    );
  }

  static Color _accent(VaultTone tone) => switch (tone) {
    VaultTone.good => Ap.glacier400,
    VaultTone.warning => Ap.ember400,
    VaultTone.blocked => Ap.rust500,
  };

  static IconData _icon(VaultTone tone) => switch (tone) {
    VaultTone.good => Icons.shield_outlined,
    VaultTone.warning => Icons.warning_amber_outlined,
    VaultTone.blocked => Icons.block_outlined,
  };
}
