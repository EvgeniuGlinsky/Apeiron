import 'dart:async';

import 'package:flutter/material.dart';

import 'chat_format.dart';
import 'chats_screen.dart' show contactLine;
import 'l10n/app_localizations.dart';
import 'src/rust/api/chat.dart';
import 'theme/tokens.dart';
import 'verify_screen.dart';

/// How often an open chat asks the DHT (`docs/transport.md` §6: 5 s for the open chat).
const chatPollEvery = Duration(seconds: 5);

/// How much of the history one screen shows. Older pages come with scrolling later.
const historyPage = 200;

/// One conversation: its history, a line to write, and the way to the safety number.
class ChatScreen extends StatefulWidget {
  const ChatScreen({super.key, required this.contact});

  final ContactItem contact;

  @override
  State<ChatScreen> createState() => _ChatScreenState();
}

class _ChatScreenState extends State<ChatScreen> {
  late ContactItem _contact = widget.contact;
  List<MessageItem> _messages = const [];
  final _input = TextEditingController();
  Timer? _poll;
  bool _polling = false;
  bool _sending = false;
  String? _error;

  /// The newest row already reported as shown.
  int _readUpTo = 0;

  @override
  void initState() {
    super.initState();
    _reload();
    _roundNow();
    _poll = Timer.periodic(chatPollEvery, (_) => _roundNow());
  }

  @override
  void dispose() {
    _poll?.cancel();
    _input.dispose();
    super.dispose();
  }

  Future<void> _reload() async {
    try {
      final messages = await chatHistory(
        contact: _contact.id,
        limit: historyPage,
      );
      final contacts = await chatContacts();
      if (!mounted) return;
      setState(() {
        _messages = messages;
        _contact = contacts.firstWhere(
          (c) => c.id == _contact.id,
          orElse: () => _contact,
        );
      });
      _markRead();
    } catch (e) {
      if (mounted) setState(() => _error = e.toString());
    }
  }

  /// What this screen shows has been seen: the list's count clears, and the contact learns it
  /// with the next round if receipts are on.
  void _markRead() {
    final newest = _messages.isEmpty ? 0 : _messages.first.id;
    if (newest <= _readUpTo) return;
    _readUpTo = newest;
    chatMarkRead(contact: _contact.id, upto: newest).catchError((_) {
      // The vault locked meanwhile: the next time the chat is shown marks it.
      _readUpTo = 0;
      return false;
    });
  }

  Future<void> _roundNow() async {
    if (_polling) return;
    _polling = true;
    try {
      if (await chatRound(contact: _contact.id)) await _reload();
    } catch (_) {
      // No network yet, or another round held the conversation: the next tick tries again.
    } finally {
      _polling = false;
    }
  }

  Future<void> _send() async {
    final text = _input.text.trim();
    if (text.isEmpty || _sending) return;
    setState(() {
      _sending = true;
      _error = null;
    });
    try {
      await chatSend(contact: _contact.id, message: text);
      _input.clear();
      await _reload();
      unawaited(_roundNow());
    } catch (e) {
      if (mounted) setState(() => _error = e.toString());
    } finally {
      if (mounted) setState(() => _sending = false);
    }
  }

  Future<void> _verify() async {
    await Navigator.of(context).push(
      MaterialPageRoute<void>(builder: (_) => VerifyScreen(contact: _contact)),
    );
    await _reload();
  }

  @override
  Widget build(BuildContext context) {
    final t = Theme.of(context).textTheme;
    final l = AppLocalizations.of(context);
    final closed = _contact.state == ContactState.taken;
    final note = switch (_contact.state) {
      ContactState.waiting => l.chatWaitingNote,
      ContactState.taken || ContactState.notAccepted => l.chatClosedNote,
      _ => null,
    };
    return Scaffold(
      appBar: AppBar(
        title: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(_contact.name, style: t.titleMedium),
            Text(
              contactLine(_contact, l),
              style: t.bodySmall?.copyWith(
                color: _contact.verified ? Ap.glacier400 : Ap.ember400,
              ),
            ),
          ],
        ),
        actions: [
          IconButton(
            tooltip: l.verifyAction,
            onPressed: _verify,
            icon: Icon(
              _contact.verified ? Icons.verified_user : Icons.gpp_maybe,
              color: _contact.verified ? Ap.glacier400 : Ap.ember400,
            ),
          ),
        ],
      ),
      body: SafeArea(
        child: Column(
          children: [
            if (note != null)
              Container(
                width: double.infinity,
                color: Ap.basalt800,
                padding: const EdgeInsets.all(Ap.s12),
                child: Text(note, style: t.bodySmall),
              ),
            if (_error != null)
              Padding(
                padding: const EdgeInsets.all(Ap.s12),
                child: Text(
                  _error!,
                  style: t.bodySmall?.copyWith(color: Ap.rust500),
                ),
              ),
            Expanded(
              child: _messages.isEmpty
                  ? Center(child: Text(l.chatEmpty, style: t.bodySmall))
                  : ListView.builder(
                      reverse: true,
                      padding: const EdgeInsets.all(Ap.s12),
                      itemCount: _messages.length,
                      itemBuilder: (_, i) =>
                          MessageBubble(message: _messages[i]),
                    ),
            ),
            const Divider(height: 1, color: Ap.stone700),
            Padding(
              padding: const EdgeInsets.fromLTRB(Ap.s12, Ap.s8, Ap.s4, Ap.s8),
              child: Row(
                children: [
                  Expanded(
                    child: TextField(
                      controller: _input,
                      enabled: !closed,
                      minLines: 1,
                      maxLines: 5,
                      textCapitalization: TextCapitalization.sentences,
                      decoration: InputDecoration(
                        hintText: l.messageHint,
                        border: InputBorder.none,
                      ),
                    ),
                  ),
                  IconButton(
                    tooltip: l.send,
                    onPressed: closed || _sending ? null : _send,
                    icon: const Icon(Icons.send, color: Ap.bone100),
                  ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// One entry of the history: my message, theirs, or a note that some were lost.
class MessageBubble extends StatelessWidget {
  const MessageBubble({super.key, required this.message});

  final MessageItem message;

  @override
  Widget build(BuildContext context) {
    final t = Theme.of(context).textTheme;
    final l = AppLocalizations.of(context);
    final m = message;
    if (m.state == MessageState.lost) {
      return Padding(
        padding: const EdgeInsets.symmetric(vertical: Ap.s8),
        child: Center(
          child: Text(
            l.msgLost,
            style: t.bodySmall?.copyWith(color: Ap.ember400),
          ),
        ),
      );
    }
    final failed = isFailed(m.state);
    return Align(
      alignment: m.mine ? Alignment.centerRight : Alignment.centerLeft,
      child: Container(
        constraints: const BoxConstraints(maxWidth: 320),
        margin: const EdgeInsets.symmetric(vertical: Ap.s4),
        padding: const EdgeInsets.all(Ap.s12),
        decoration: BoxDecoration(
          color: m.mine ? Ap.stone700 : Ap.basalt800,
          border: Border.all(color: failed ? Ap.rust500 : Ap.stone700),
        ),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.end,
          children: [
            Align(
              alignment: Alignment.centerLeft,
              child: SelectableText(
                m.text,
                style: t.bodyMedium?.copyWith(color: Ap.bone100),
              ),
            ),
            const SizedBox(height: Ap.s4),
            Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                Text(
                  clockTime(m.at),
                  style: t.bodySmall?.copyWith(
                    fontSize: 11,
                    color: failed ? Ap.rust500 : Ap.fog400,
                  ),
                ),
                if (m.mine) ...[
                  const SizedBox(width: Ap.s4),
                  StatusIcon(state: m.state),
                ],
              ],
            ),
          ],
        ),
      ),
    );
  }
}

bool isFailed(MessageState s) =>
    s == MessageState.notDelivered || s == MessageState.addressTaken;

/// Where my message stands, as a mark: a clock, one grey tick (on the network), two grey
/// (on their phone), two in glacier blue (they were shown it), red (not delivered). The words
/// stay in the tooltip and for screen readers.
class StatusIcon extends StatelessWidget {
  const StatusIcon({super.key, required this.state, this.size = 14});

  final MessageState state;
  final double size;

  @override
  Widget build(BuildContext context) {
    final l = AppLocalizations.of(context);
    final (icon, color, words) = switch (state) {
      MessageState.queued => (Icons.schedule, Ap.fog400, l.msgQueued),
      MessageState.sent => (Icons.check, Ap.fog400, l.msgSent),
      MessageState.delivered => (Icons.done_all, Ap.fog400, l.msgDelivered),
      MessageState.read => (Icons.done_all, Ap.glacier400, l.msgRead),
      MessageState.notDelivered => (
        Icons.error_outline,
        Ap.rust500,
        l.msgNotDelivered,
      ),
      MessageState.addressTaken => (
        Icons.error_outline,
        Ap.rust500,
        l.msgAddressTaken,
      ),
      MessageState.received || MessageState.lost => (null, Ap.fog400, ''),
    };
    if (icon == null) return const SizedBox.shrink();
    return Tooltip(
      message: words,
      child: Icon(icon, size: size, color: color, semanticLabel: words),
    );
  }
}
