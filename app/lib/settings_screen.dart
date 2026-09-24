import 'package:flutter/material.dart';

import 'change_pin_screen.dart';
import 'check_screen.dart';
import 'identity_view.dart';
import 'l10n/app_localizations.dart';
import 'lock_policy.dart';
import 'src/rust/api/chat.dart';
import 'src/rust/api/identity.dart';
import 'src/rust/api/pin.dart';
import 'theme/tokens.dart';

/// Settings: the PIN pad and the PIN, the self-check, and the owner's own identity.
class SettingsScreen extends StatefulWidget {
  const SettingsScreen({
    super.key,
    required this.identity,
    required this.policy,
  });

  final PublicIdentityView identity;
  final LockPolicy policy;

  @override
  State<SettingsScreen> createState() => _SettingsScreenState();
}

class _SettingsScreenState extends State<SettingsScreen> {
  PinPadPrefs? _prefs;
  bool? _receipts;

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
    final prefs = await pinPadPrefs();
    final receipts = await chatReadReceipts().catchError((Object _) => true);
    if (mounted) {
      setState(() {
        _prefs = prefs;
        _receipts = receipts;
      });
    }
  }

  Future<void> _setReceipts(bool on) async {
    await chatSetReadReceipts(enabled: on);
    await _load();
  }

  Future<void> _scrambled(bool on) async {
    await setPinPadPrefs(digits: _prefs?.digits ?? 0, scrambled: on);
    await _load();
  }

  Future<void> _push(Widget screen) async {
    await Navigator.of(
      context,
    ).push(MaterialPageRoute<void>(builder: (_) => screen));
    await _load();
  }

  @override
  Widget build(BuildContext context) {
    final t = Theme.of(context).textTheme;
    final l = AppLocalizations.of(context);
    final prefs = _prefs;
    return Scaffold(
      appBar: AppBar(title: Text(l.settingsTitle, style: t.labelLarge)),
      body: SafeArea(
        child: ListView(
          padding: const EdgeInsets.symmetric(vertical: Ap.s12),
          children: [
            Padding(
              padding: const EdgeInsets.fromLTRB(Ap.s16, Ap.s8, Ap.s16, 0),
              child: Text(l.settingsPin, style: t.labelMedium),
            ),
            SwitchListTile(
              value: prefs?.scrambled ?? true,
              onChanged: prefs == null ? null : _scrambled,
              title: Text(l.settingsKeyboard),
              subtitle: Text(l.settingsKeyboardNote, style: t.bodySmall),
            ),
            ListTile(
              title: Text(l.settingsChangePin),
              subtitle: Text(
                [
                  if ((prefs?.digits ?? 0) > 0) l.pinDigits(prefs!.digits),
                  l.settingsChangePinNote,
                ].join(' · '),
                style: t.bodySmall,
              ),
              trailing: const Icon(Icons.chevron_right, color: Ap.fog400),
              onTap: () => _push(const ChangePinScreen()),
            ),
            const Divider(color: Ap.stone700),
            SwitchListTile(
              value: _receipts ?? true,
              onChanged: _receipts == null ? null : _setReceipts,
              title: Text(l.settingsReadReceipts),
              subtitle: Text(l.settingsReadReceiptsNote, style: t.bodySmall),
            ),
            const Divider(color: Ap.stone700),
            ListTile(
              leading: const Icon(Icons.fact_check_outlined, color: Ap.fog400),
              title: Text(l.settingsSelfCheck),
              trailing: const Icon(Icons.chevron_right, color: Ap.fog400),
              onTap: () => _push(const CheckScreen()),
            ),
            const Divider(color: Ap.stone700),
            Padding(
              padding: const EdgeInsets.all(Ap.s20),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  Text(l.settingsIdentity, style: t.labelMedium),
                  const SizedBox(height: Ap.s16),
                  IdentityView(identity: widget.identity),
                  const SizedBox(height: Ap.s40),
                  HonestNote(policy: widget.policy),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}
