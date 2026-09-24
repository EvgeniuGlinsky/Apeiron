import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'chat_format.dart';
import 'l10n/app_localizations.dart';
import 'src/rust/api/chat.dart';
import 'theme/tokens.dart';

/// Making an invitation, or looking at one that waits: the text to send, its expiry, and why
/// the safety number has to be compared afterwards.
class InviteScreen extends StatefulWidget {
  const InviteScreen({super.key, this.existing});

  final InvitationItem? existing;

  @override
  State<InviteScreen> createState() => _InviteScreenState();
}

class _InviteScreenState extends State<InviteScreen> {
  final _name = TextEditingController();
  late InvitationItem? _invitation = widget.existing;
  bool _busy = false;
  String? _error;

  @override
  void dispose() {
    _name.dispose();
    super.dispose();
  }

  Future<void> _create() async {
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final made = await chatCreateInvitation(name: _name.text);
      if (mounted) setState(() => _invitation = made);
    } catch (e) {
      if (mounted) setState(() => _error = e.toString());
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _withdraw(InvitationItem i) async {
    try {
      await chatCancelInvitation(id: i.id);
      if (mounted) Navigator.of(context).pop();
    } catch (e) {
      if (mounted) setState(() => _error = e.toString());
    }
  }

  @override
  Widget build(BuildContext context) {
    final t = Theme.of(context).textTheme;
    final l = AppLocalizations.of(context);
    final i = _invitation;
    return Scaffold(
      appBar: AppBar(title: Text(l.inviteTitle, style: t.labelLarge)),
      body: SafeArea(
        child: ListView(
          padding: const EdgeInsets.all(Ap.s20),
          children: [
            if (_error != null) ...[
              Text(_error!, style: t.bodySmall?.copyWith(color: Ap.rust500)),
              const SizedBox(height: Ap.s16),
            ],
            if (i == null) ...[
              TextField(
                controller: _name,
                decoration: InputDecoration(labelText: l.inviteName),
                textCapitalization: TextCapitalization.words,
              ),
              const SizedBox(height: Ap.s20),
              FilledButton(
                onPressed: _busy ? null : _create,
                child: Text(l.inviteCreate),
              ),
            ] else ...[
              Text(i.name, style: t.labelLarge),
              const SizedBox(height: Ap.s12),
              Container(
                padding: const EdgeInsets.all(Ap.s16),
                decoration: BoxDecoration(
                  color: Ap.basalt800,
                  border: Border.all(color: Ap.stone700),
                ),
                child: SelectableText(
                  i.text,
                  style: Ap.mono(size: 12, color: Ap.bone100),
                ),
              ),
              const SizedBox(height: Ap.s12),
              FilledButton.icon(
                onPressed: () {
                  Clipboard.setData(ClipboardData(text: i.text));
                  ScaffoldMessenger.of(
                    context,
                  ).showSnackBar(SnackBar(content: Text(l.copied)));
                },
                icon: const Icon(Icons.content_copy),
                label: Text(l.copy),
              ),
              const SizedBox(height: Ap.s20),
              Text(l.inviteExpires(dayDate(i.expiresDay)), style: t.bodySmall),
              const SizedBox(height: Ap.s8),
              Text(l.inviteBody, style: t.bodySmall),
              const SizedBox(height: Ap.s28),
              OutlinedButton(
                onPressed: () => _withdraw(i),
                child: Text(l.inviteWithdraw),
              ),
            ],
          ],
        ),
      ),
    );
  }
}
