import 'package:flutter/material.dart';

import 'src/rust/api/vault.dart';
import 'theme/tokens.dart';
import 'vault_status.dart';

/// Состояние хранилища ключа — как оно выглядит на экране.
///
/// Показывается всегда, а не только при беде. R-002 требует честного
/// предупреждения там, где аппаратного хранилища нет; но и обратное верно:
/// человек должен видеть, на чём держится защита, не дожидаясь, пока она
/// откажет.
class VaultPanel extends StatelessWidget {
  const VaultPanel({
    super.key,
    required this.status,
    this.onRetry,
    this.onFreshStart,
  });

  final VaultStatus status;

  /// Повторить попытку. Данные при этом целы.
  final VoidCallback? onRetry;

  /// Начать заново. Предлагается только когда ключ действительно исчез.
  final VoidCallback? onFreshStart;

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
          if (onRetry != null || (onFreshStart != null &&
              mayOfferFreshStart(status))) ...[
            const SizedBox(height: Ap.s16),
            Row(
              children: [
                if (onRetry != null)
                  FilledButton(
                    onPressed: onRetry,
                    child: const Text('ПОВТОРИТЬ'),
                  ),
                // Кнопка «начать заново» существует ровно в одном положении.
                // Предложить стереть всё при преходящем сбое прошивки значило
                // бы уничтожить переписку владельца за него.
                if (onFreshStart != null && mayOfferFreshStart(status)) ...[
                  const SizedBox(width: Ap.s12),
                  OutlinedButton(
                    onPressed: onFreshStart,
                    child: const Text('НАЧАТЬ ЗАНОВО'),
                  ),
                ],
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
