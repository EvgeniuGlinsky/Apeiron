import 'dart:async';

import 'package:flutter/material.dart';

import 'accept_screen.dart';
import 'chat_format.dart';
import 'chat_screen.dart';
import 'invite_screen.dart';
import 'l10n/app_localizations.dart';
import 'lock_policy.dart';
import 'raven.dart';
import 'settings_screen.dart';
import 'src/rust/api/chat.dart';
import 'src/rust/api/identity.dart';
import 'theme/tokens.dart';
import 'wordmark.dart';

/// How often the list asks the DHT: every open invitation's inbox and every conversation.
/// The open chat asks more often on its own (`ChatScreen`), and while another screen is on top
/// the list does not ask at all.
///
/// 15 s, not the 60 s of the first outline: a message reaches the DHT in a few seconds and is
/// found in 1–4 s, so the minute was nearly the whole of the wait. Faster costs one round of
/// lookups per contact each time, which the network sees as the pattern of an open app (§7 of
/// `docs/transport.md`) — which it is.
const listPollEvery = Duration(seconds: 15);

/// The home screen once the vault is open: conversations and the invitations waiting.
class ChatsHome extends StatefulWidget {
  const ChatsHome({
    super.key,
    required this.identity,
    required this.policy,
    required this.onLock,
  });

  final PublicIdentityView identity;
  final LockPolicy policy;
  final VoidCallback onLock;

  @override
  State<ChatsHome> createState() => _ChatsHomeState();
}

class _ChatsHomeState extends State<ChatsHome> {
  List<ContactItem> _contacts = const [];
  List<InvitationItem> _invitations = const [];
  String? _error;
  Timer? _poll;
  bool _polling = false;

  @override
  void initState() {
    super.initState();
    _reload();
    _pollNow();
    _startPolling();
  }

  void _startPolling() {
    _poll?.cancel();
    _poll = Timer.periodic(listPollEvery, (_) => _pollNow());
  }

  @override
  void dispose() {
    _poll?.cancel();
    super.dispose();
  }

  Future<void> _reload() async {
    try {
      final contacts = await chatContacts();
      final invitations = await chatInvitations();
      if (mounted) {
        setState(() {
          _contacts = contacts;
          _invitations = invitations;
          _error = null;
        });
      }
    } catch (e) {
      if (mounted) setState(() => _error = e.toString());
    }
  }

  Future<void> _pollNow() async {
    if (_polling) return;
    _polling = true;
    try {
      if (await chatPoll()) await _reload();
    } catch (_) {
      // No network, or the vault locked meanwhile: the next tick tries again.
    } finally {
      _polling = false;
    }
  }

  Future<void> _open(Widget screen) async {
    _poll?.cancel();
    await Navigator.of(
      context,
    ).push(MaterialPageRoute<void>(builder: (_) => screen));
    if (!mounted) return;
    _startPolling();
    await _reload();
    unawaited(_pollNow());
  }

  void _newContact() {
    final l = AppLocalizations.of(context);
    showModalBottomSheet<void>(
      context: context,
      backgroundColor: Ap.basalt800,
      builder: (sheet) => SafeArea(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            ListTile(
              leading: const Icon(Icons.outgoing_mail, color: Ap.fog400),
              title: Text(l.invite),
              onTap: () {
                Navigator.of(sheet).pop();
                _open(const InviteScreen());
              },
            ),
            ListTile(
              leading: const Icon(Icons.move_to_inbox, color: Ap.fog400),
              title: Text(l.acceptInvitation),
              onTap: () {
                Navigator.of(sheet).pop();
                _open(const AcceptScreen());
              },
            ),
          ],
        ),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final l = AppLocalizations.of(context);
    return Scaffold(
      appBar: AppBar(
        titleSpacing: Ap.s20,
        title: const Row(
          children: [
            ApeironRaven(size: 26),
            SizedBox(width: Ap.s12),
            ApeironWordmark(height: 19),
          ],
        ),
        actions: [
          IconButton(
            tooltip: l.settingsTooltip,
            onPressed: () => _open(
              SettingsScreen(identity: widget.identity, policy: widget.policy),
            ),
            icon: const Icon(Icons.settings_outlined, color: Ap.fog400),
          ),
          IconButton(
            tooltip: l.lockTooltip,
            onPressed: widget.onLock,
            icon: const Icon(Icons.lock_outline, color: Ap.fog400),
          ),
          const SizedBox(width: Ap.s8),
        ],
      ),
      floatingActionButton: FloatingActionButton(
        onPressed: _newContact,
        backgroundColor: Ap.bone100,
        foregroundColor: Ap.basalt900,
        child: const Icon(Icons.add),
      ),
      body: SafeArea(
        child: RefreshIndicator(
          onRefresh: () async {
            await _pollNow();
            await _reload();
          },
          child: ChatsList(
            contacts: _contacts,
            invitations: _invitations,
            error: _error,
            onOpenContact: (c) => _open(ChatScreen(contact: c)),
            onOpenInvitation: (i) => _open(InviteScreen(existing: i)),
          ),
        ),
      ),
    );
  }
}

/// The list itself, without Rust: what the screen shows for given data.
class ChatsList extends StatelessWidget {
  const ChatsList({
    super.key,
    required this.contacts,
    required this.invitations,
    required this.onOpenContact,
    required this.onOpenInvitation,
    this.error,
    this.now,
  });

  final List<ContactItem> contacts;
  final List<InvitationItem> invitations;
  final ValueChanged<ContactItem> onOpenContact;
  final ValueChanged<InvitationItem> onOpenInvitation;
  final String? error;

  /// The moment the times are told against; the clock when not given (tests give it).
  final DateTime? now;

  @override
  Widget build(BuildContext context) {
    final t = Theme.of(context).textTheme;
    final l = AppLocalizations.of(context);
    return ListView(
      padding: const EdgeInsets.symmetric(vertical: Ap.s12),
      children: [
        if (error != null)
          Padding(
            padding: const EdgeInsets.all(Ap.s16),
            child: Text(
              error!,
              style: t.bodySmall?.copyWith(color: Ap.rust500),
            ),
          ),
        if (invitations.isNotEmpty) ...[
          _Section(l.invitationsWaiting),
          for (final i in invitations)
            ListTile(
              leading: const Icon(Icons.hourglass_empty, color: Ap.fog400),
              title: Text(i.name.isEmpty ? '—' : i.name),
              subtitle: Text(
                l.invitationUntil(dayDate(i.expiresDay)),
                style: t.bodySmall,
              ),
              onTap: () => onOpenInvitation(i),
            ),
          const Divider(color: Ap.stone700),
        ],
        if (contacts.isEmpty && invitations.isEmpty)
          Padding(
            padding: const EdgeInsets.all(Ap.s28),
            child: Column(
              children: [
                const SizedBox(height: Ap.s40),
                const ApeironRaven(size: 92, color: Ap.stone600),
                const SizedBox(height: Ap.s28),
                Text(
                  l.chatsEmptyTitle,
                  style: t.labelLarge?.copyWith(color: Ap.bone100),
                  textAlign: TextAlign.center,
                ),
                const SizedBox(height: Ap.s12),
                Text(
                  l.chatsEmptyBody,
                  style: t.bodySmall,
                  textAlign: TextAlign.center,
                ),
              ],
            ),
          ),
        for (final c in contacts)
          ListTile(
            leading: CircleAvatar(
              backgroundColor: Ap.basalt800,
              child: Text(
                c.name.isEmpty ? '?' : c.name.characters.first.toUpperCase(),
                style: t.labelLarge,
              ),
            ),
            title: Row(
              children: [
                Flexible(
                  child: Text(
                    c.name.isEmpty ? '—' : c.name,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                  ),
                ),
                const SizedBox(width: Ap.s4),
                // Not verified is said wherever the contact is shown (§8).
                Icon(
                  c.verified ? Icons.verified_user : Icons.gpp_maybe,
                  size: 14,
                  color: c.verified ? Ap.glacier400 : Ap.ember400,
                  semanticLabel: c.verified
                      ? l.contactVerified
                      : l.contactNotVerified,
                ),
              ],
            ),
            subtitle: _Preview(contact: c),
            trailing: _When(contact: c, now: now ?? DateTime.now()),
            onTap: () => onOpenContact(c),
          ),
      ],
    );
  }
}

/// Under the name: the newest entry of a live conversation, or where the contact stands.
class _Preview extends StatelessWidget {
  const _Preview({required this.contact});

  final ContactItem contact;

  @override
  Widget build(BuildContext context) {
    final t = Theme.of(context).textTheme;
    final l = AppLocalizations.of(context);
    final c = contact;
    final last = c.last;
    if (last == null || c.state != ContactState.live) {
      return Text(
        contactLine(c, l),
        style: t.bodySmall?.copyWith(
          color: c.verified && c.state == ContactState.live
              ? Ap.glacier400
              : Ap.fog400,
        ),
      );
    }
    final line = last.text.replaceAll(RegExp(r'\s+'), ' ');
    final text = last.state == MessageState.lost
        ? l.msgLost
        : last.mine
        ? l.previewMine(line)
        : line;
    return Row(
      children: [
        if (last.mine) ...[
          StatusIcon(state: last.state, size: 13),
          const SizedBox(width: Ap.s4),
        ],
        Expanded(
          child: Text(
            text,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: t.bodySmall?.copyWith(
              color: c.unread > 0 ? Ap.bone100 : Ap.fog400,
            ),
          ),
        ),
      ],
    );
  }
}

/// On the right: when the newest entry was, and how many are unread.
class _When extends StatelessWidget {
  const _When({required this.contact, required this.now});

  final ContactItem contact;
  final DateTime now;

  @override
  Widget build(BuildContext context) {
    final t = Theme.of(context).textTheme;
    final last = contact.last;
    final unread = contact.unread;
    return Column(
      mainAxisAlignment: MainAxisAlignment.center,
      crossAxisAlignment: CrossAxisAlignment.end,
      mainAxisSize: MainAxisSize.min,
      children: [
        if (last != null)
          Text(
            listTime(last.at, now),
            style: t.bodySmall?.copyWith(
              fontSize: 11,
              color: unread > 0 ? Ap.bone100 : Ap.fog400,
            ),
          ),
        if (unread > 0) ...[
          const SizedBox(height: Ap.s4),
          Container(
            constraints: const BoxConstraints(minWidth: 20),
            padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 1),
            decoration: BoxDecoration(
              color: Ap.bone100,
              borderRadius: BorderRadius.circular(2),
            ),
            child: Text(
              unread > 99 ? '99+' : '$unread',
              textAlign: TextAlign.center,
              style: t.labelSmall?.copyWith(
                color: Ap.basalt900,
                fontWeight: FontWeight.w700,
              ),
            ),
          ),
        ],
      ],
    );
  }
}

/// What the list says under a contact's name.
String contactLine(ContactItem c, AppLocalizations l) => switch (c.state) {
  ContactState.waiting => l.contactWaiting,
  ContactState.notAccepted => l.contactNotAccepted,
  ContactState.taken => l.contactTaken,
  ContactState.damaged => l.contactDamaged,
  ContactState.live => c.verified ? l.contactVerified : l.contactNotVerified,
};

class _Section extends StatelessWidget {
  const _Section(this.title);

  final String title;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.fromLTRB(Ap.s16, Ap.s8, Ap.s16, Ap.s4),
    child: Text(title, style: Theme.of(context).textTheme.labelMedium),
  );
}
