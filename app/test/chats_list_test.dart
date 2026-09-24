/// The conversations list: an empty state that says what to do, a contact's line that says
/// where it stands — never "verified" unless the owner verified it — and a live one's newest
/// entry with its time and the unread count.
library;

import 'package:apeiron/chat_format.dart';
import 'package:apeiron/chats_screen.dart';
import 'package:apeiron/l10n/app_localizations.dart';
import 'package:apeiron/src/rust/api/chat.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

Widget host(Widget child) => MaterialApp(
  locale: const Locale('en'),
  localizationsDelegates: AppLocalizations.localizationsDelegates,
  supportedLocales: AppLocalizations.supportedLocales,
  home: Scaffold(body: child),
);

void main() {
  testWidgets(
    'empty, it says how to begin; filled, it says where each one stands',
    (tester) async {
      final l = await AppLocalizations.delegate.load(const Locale('en'));
      await tester.pumpWidget(
        host(
          ChatsList(
            contacts: const [],
            invitations: const [],
            onOpenContact: (_) {},
            onOpenInvitation: (_) {},
          ),
        ),
      );
      expect(find.text(l.chatsEmptyTitle), findsOneWidget);

      await tester.pumpWidget(
        host(
          ChatsList(
            contacts: const [
              ContactItem(
                id: 1,
                name: 'Bob',
                state: ContactState.live,
                verified: false,
                unread: 0,
              ),
              ContactItem(
                id: 2,
                name: 'Carol',
                state: ContactState.waiting,
                verified: false,
                unread: 0,
              ),
              ContactItem(
                id: 4,
                name: 'Erin',
                state: ContactState.live,
                verified: true,
                last: MessageItem(
                  id: 9,
                  mine: true,
                  state: MessageState.read,
                  at: 1790251200,
                  text: 'see\nyou',
                ),
                unread: 120,
              ),
            ],
            invitations: const [
              InvitationItem(
                id: 3,
                name: 'Dave',
                expiresDay: 20727,
                text: 'apeiron:x',
              ),
            ],
            onOpenContact: (_) {},
            onOpenInvitation: (_) {},
            now: DateTime(2027),
          ),
        ),
      );
      expect(find.text(l.chatsEmptyTitle), findsNothing);
      // A preview on one line, mine marked as such, its time, and a count that does not grow
      // out of the row.
      expect(find.text(l.previewMine('see you')), findsOneWidget);
      expect(find.text('99+'), findsOneWidget);
      expect(find.text(listTime(1790251200, DateTime(2027))), findsOneWidget);
      expect(listTime(1790251200, DateTime(2027)), '24.09.2026');
      expect(find.text(l.contactNotVerified), findsOneWidget);
      expect(find.text(l.contactWaiting), findsOneWidget);
      expect(find.text(l.invitationUntil(dayDate(20727))), findsOneWidget);
      expect(dayDate(20727), '01.10.2026');
    },
  );
}
