import 'package:flutter/material.dart';

import 'identity_view.dart';
import 'l10n/app_localizations.dart';
import 'src/rust/api/chat.dart';
import 'theme/tokens.dart';

/// The safety number with a contact, read aloud in steps, and the owner's own act of saying
/// whether it matched (`docs/transport.md` §8). Nothing marks a contact verified but the
/// "match" button.
///
/// "They do not match" is a branch of its own, not the absence of a tap: it takes the mark back
/// and shows the two fingerprints the number is made of, so the two people can tell a replaced
/// key (a fingerprint differs) from a fault of the app (both agree, the numbers still differ).
class VerifyScreen extends StatefulWidget {
  const VerifyScreen({super.key, required this.contact});

  final ContactItem contact;

  @override
  State<VerifyScreen> createState() => _VerifyScreenState();
}

class _VerifyScreenState extends State<VerifyScreen> {
  VerificationItem? _v;
  String? _error;
  late bool _verified = widget.contact.verified;
  bool _mismatch = false;

  @override
  void initState() {
    super.initState();
    chatVerification(contact: widget.contact.id).then(
      (v) => mounted ? setState(() => _v = v) : null,
      onError: (Object e) =>
          mounted ? setState(() => _error = e.toString()) : null,
    );
  }

  Future<bool> _set(bool verified) async {
    try {
      await chatSetVerified(contact: widget.contact.id, verified: verified);
      if (mounted) setState(() => _verified = verified);
      return true;
    } catch (e) {
      if (mounted) setState(() => _error = e.toString());
      return false;
    }
  }

  Future<void> _match() async {
    if (await _set(true) && mounted) Navigator.of(context).pop();
  }

  Future<void> _noMatch() async {
    if (_verified && !await _set(false)) return;
    if (mounted) setState(() => _mismatch = true);
  }

  @override
  Widget build(BuildContext context) {
    final t = Theme.of(context).textTheme;
    final l = AppLocalizations.of(context);
    final name = widget.contact.name;
    final v = _v;
    return Scaffold(
      appBar: AppBar(title: Text(l.verifyTitle, style: t.labelLarge)),
      body: SafeArea(
        child: ListView(
          padding: const EdgeInsets.all(Ap.s20),
          children: [
            if (_error != null) ...[
              Text(_error!, style: t.bodySmall?.copyWith(color: Ap.rust500)),
              const SizedBox(height: Ap.s16),
            ],
            if (_mismatch && v != null)
              _Mismatch(
                name: name,
                verification: v,
                onBack: () => setState(() => _mismatch = false),
              )
            else ...[
              Text(l.verifyBody(name), style: t.bodySmall),
              const SizedBox(height: Ap.s20),
              if (v != null) DigitGrid(digits: v.safetyNumber),
              const SizedBox(height: Ap.s28),
              _Step(n: 1, text: l.verifyStep1(name)),
              _Step(n: 2, text: l.verifyStep2(name)),
              _Step(n: 3, text: l.verifyStep3),
              const SizedBox(height: Ap.s20),
              if (_verified) ...[
                Text(
                  l.verifiedByYou,
                  style: t.labelMedium?.copyWith(color: Ap.glacier400),
                ),
                const SizedBox(height: Ap.s12),
                OutlinedButton(
                  onPressed: () => _set(false),
                  child: Text(l.verifyUndo),
                ),
              ] else
                FilledButton(
                  onPressed: v == null ? null : _match,
                  child: Text(l.verifyConfirm),
                ),
              const SizedBox(height: Ap.s12),
              OutlinedButton(
                onPressed: v == null ? null : _noMatch,
                style: OutlinedButton.styleFrom(
                  foregroundColor: Ap.rust500,
                  side: const BorderSide(color: Ap.rust500),
                ),
                child: Text(l.verifyMismatch),
              ),
            ],
          ],
        ),
      ),
    );
  }
}

class _Step extends StatelessWidget {
  const _Step({required this.n, required this.text});

  final int n;
  final String text;

  @override
  Widget build(BuildContext context) {
    final t = Theme.of(context).textTheme;
    return Padding(
      padding: const EdgeInsets.only(bottom: Ap.s12),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          SizedBox(
            width: Ap.s28,
            child: Text('$n', style: Ap.mono(size: 15, color: Ap.ember400)),
          ),
          Expanded(
            child: Text(text, style: t.bodyMedium?.copyWith(color: Ap.bone100)),
          ),
        ],
      ),
    );
  }
}

class _Mismatch extends StatelessWidget {
  const _Mismatch({
    required this.name,
    required this.verification,
    required this.onBack,
  });

  final String name;
  final VerificationItem verification;
  final VoidCallback onBack;

  @override
  Widget build(BuildContext context) {
    final t = Theme.of(context).textTheme;
    final l = AppLocalizations.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Text(l.mismatchTitle, style: t.labelLarge?.copyWith(color: Ap.rust500)),
        const SizedBox(height: Ap.s12),
        Text(l.mismatchBody, style: t.bodySmall),
        const SizedBox(height: Ap.s20),
        Text(l.mismatchTheirs(name), style: t.labelMedium),
        const SizedBox(height: Ap.s8),
        DigitGrid.plain(digits: verification.theirFingerprint),
        const SizedBox(height: Ap.s20),
        Text(l.mismatchMine(name), style: t.labelMedium),
        const SizedBox(height: Ap.s8),
        DigitGrid.plain(digits: verification.myFingerprint),
        const SizedBox(height: Ap.s20),
        Text(l.mismatchVerdict, style: t.bodySmall),
        const SizedBox(height: Ap.s20),
        OutlinedButton(onPressed: onBack, child: Text(l.mismatchBack)),
      ],
    );
  }
}
