import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'l10n/app_localizations.dart';
import 'src/rust/api/chat.dart';
import 'theme/tokens.dart';

/// Accepting an invitation someone sent: its text, a name for them, and a first message.
class AcceptScreen extends StatefulWidget {
  const AcceptScreen({super.key});

  @override
  State<AcceptScreen> createState() => _AcceptScreenState();
}

class _AcceptScreenState extends State<AcceptScreen> {
  final _text = TextEditingController();
  final _name = TextEditingController();
  final _first = TextEditingController();
  bool _busy = false;
  String? _error;

  @override
  void dispose() {
    _text.dispose();
    _name.dispose();
    _first.dispose();
    super.dispose();
  }

  Future<void> _paste() async {
    final data = await Clipboard.getData(Clipboard.kTextPlain);
    final text = data?.text;
    if (text != null) setState(() => _text.text = text);
  }

  Future<void> _accept() async {
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      await chatAcceptInvitation(
        textOfInvitation: _text.text,
        name: _name.text,
        firstText: _first.text,
      );
      if (mounted) Navigator.of(context).pop();
    } catch (e) {
      if (mounted) setState(() => _error = e.toString());
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final t = Theme.of(context).textTheme;
    final l = AppLocalizations.of(context);
    return Scaffold(
      appBar: AppBar(title: Text(l.acceptInvitation, style: t.labelLarge)),
      body: SafeArea(
        child: ListView(
          padding: const EdgeInsets.all(Ap.s20),
          children: [
            if (_error != null) ...[
              Text(_error!, style: t.bodySmall?.copyWith(color: Ap.rust500)),
              const SizedBox(height: Ap.s16),
            ],
            TextField(
              controller: _text,
              minLines: 3,
              maxLines: 6,
              style: Ap.mono(size: 12),
              decoration: InputDecoration(
                labelText: l.acceptPaste,
                suffixIcon: IconButton(
                  onPressed: _paste,
                  icon: const Icon(Icons.content_paste, color: Ap.fog400),
                ),
              ),
            ),
            const SizedBox(height: Ap.s16),
            TextField(
              controller: _name,
              decoration: InputDecoration(labelText: l.acceptName),
              textCapitalization: TextCapitalization.words,
            ),
            const SizedBox(height: Ap.s16),
            TextField(
              controller: _first,
              maxLines: 3,
              decoration: InputDecoration(labelText: l.acceptFirst),
            ),
            const SizedBox(height: Ap.s20),
            FilledButton(
              onPressed: _busy ? null : _accept,
              child: Text(l.acceptButton),
            ),
            const SizedBox(height: Ap.s12),
            Text(l.acceptNote, style: t.bodySmall),
          ],
        ),
      ),
    );
  }
}
