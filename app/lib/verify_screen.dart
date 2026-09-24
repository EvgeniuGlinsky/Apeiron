import 'package:flutter/material.dart';

import 'identity_view.dart';
import 'l10n/app_localizations.dart';
import 'src/rust/api/chat.dart';
import 'theme/tokens.dart';

/// The safety number with a contact, and the owner's own act of saying it matched
/// (`docs/transport.md` §8). Nothing marks a contact verified but this button.
class VerifyScreen extends StatefulWidget {
  const VerifyScreen({super.key, required this.contact});

  final ContactItem contact;

  @override
  State<VerifyScreen> createState() => _VerifyScreenState();
}

class _VerifyScreenState extends State<VerifyScreen> {
  String? _number;
  String? _error;

  @override
  void initState() {
    super.initState();
    chatSafetyNumber(contact: widget.contact.id).then(
      (n) => mounted ? setState(() => _number = n) : null,
      onError: (Object e) =>
          mounted ? setState(() => _error = e.toString()) : null,
    );
  }

  Future<void> _mark(bool verified) async {
    try {
      await chatSetVerified(contact: widget.contact.id, verified: verified);
      if (mounted) Navigator.of(context).pop();
    } catch (e) {
      if (mounted) setState(() => _error = e.toString());
    }
  }

  @override
  Widget build(BuildContext context) {
    final t = Theme.of(context).textTheme;
    final l = AppLocalizations.of(context);
    final number = _number;
    return Scaffold(
      appBar: AppBar(title: Text(l.verifyTitle, style: t.labelLarge)),
      body: SafeArea(
        child: ListView(
          padding: const EdgeInsets.all(Ap.s20),
          children: [
            Text(l.verifyBody(widget.contact.name), style: t.bodySmall),
            const SizedBox(height: Ap.s20),
            if (_error != null)
              Text(_error!, style: t.bodySmall?.copyWith(color: Ap.rust500)),
            if (number != null) DigitGrid(digits: number),
            const SizedBox(height: Ap.s28),
            if (widget.contact.verified) ...[
              Text(
                l.verifiedByYou,
                style: t.labelMedium?.copyWith(color: Ap.glacier400),
              ),
              const SizedBox(height: Ap.s12),
              OutlinedButton(
                onPressed: () => _mark(false),
                child: Text(l.verifyUndo),
              ),
            ] else
              FilledButton(
                onPressed: number == null ? null : () => _mark(true),
                child: Text(l.verifyConfirm),
              ),
          ],
        ),
      ),
    );
  }
}
