/// What to tell a person about the key vault.
///
/// The logic is split out of the widget following [lock_policy.dart] and for
/// the same reason: it is checked by tests, while the screen cannot be checked
/// at all without a built native library. The rule from there carries over
/// literally — **the text must match the behaviour**.
///
/// The wording here is not decoration. The table in `docs/threat-log.md`
/// explicitly forbids promising more than has been done, and this task covers
/// less than it seems: it gives binding to the device and makes a copied data
/// directory useless, but does **not** protect against a phone taken away
/// while unlocked. The PIN will cover that, and until then that is what must
/// be said.
library;

import 'src/rust/api/vault.dart' show VaultState, VaultStatus;

/// How serious the state is.
enum VaultTone {
  /// Everything as designed.
  good,

  /// Works, but weaker than promised. Staying silent about it is not allowed.
  warning,

  /// Cannot go further without the person's decision.
  blocked,
}

/// Ready-made text for the screen.
class VaultMessage {
  const VaultMessage({
    required this.tone,
    required this.title,
    required this.detail,
    required this.action,
  });

  /// Tone: green, warning or dead end.
  final VaultTone tone;

  /// A one-line heading.
  final String title;

  /// Explanation. Neither softens nor over-promises.
  final String detail;

  /// What the person can do. Empty string if there is nothing to do.
  final String action;
}

/// Level name together with the raw number.
///
/// The raw number is shown alongside on purpose: the system has five values,
/// not three, and an unfamiliar one must be visible rather than replaced by
/// the nearest familiar one.
String levelLabel(VaultStatus status) =>
    '${status.levelName} (${status.levelRaw})';

/// What to show for the current state of the vault.
VaultMessage describeVault(VaultStatus status) {
  switch (status.state) {
    case VaultState.keyGone:
      return VaultMessage(
        tone: VaultTone.blocked,
        title: 'Ключ этого телефона исчез',
        detail:
            'Ключ, которым зашифровано хранилище, лежал в защищённом модуле '
            'этого телефона и оттуда пропал. Так бывает после обновления '
            'прошивки, при снятии блокировки экрана и при восстановлении '
            'данных из резервной копии. Расшифровать переписку нельзя ничем: '
            'ключ не выгружался наружу — в этом и был смысл.',
        action: 'Начать заново. Прежняя переписка не вернётся.',
      );

    case VaultState.retry:
      return VaultMessage(
        tone: VaultTone.warning,
        title: 'Хранилище сейчас недоступно',
        detail: status.message.isEmpty
            ? 'Защищённый модуль устройства не ответил.'
            : status.message,
        action: 'Данные целы. Повторите попытку.',
      );

    case VaultState.unavailable:
      return VaultMessage(
        tone: VaultTone.blocked,
        title: 'На этой платформе хранилища нет',
        detail:
            'Аппаратного хранилища ключей здесь не существует, а класть ключ '
            'в файл рядом с данными и называть это защитой — неправда. '
            'Сборка для этой платформы заморожена.',
        action: '',
      );

    case VaultState.locked:
      return const VaultMessage(
        tone: VaultTone.warning,
        title: 'Заперто',
        detail:
            'Ключ хранилища затёрт в памяти. Чтобы открыть его заново, нужен '
            'защищённый модуль устройства, а на запертом телефоне он работать '
            'откажется.',
        action: 'Разблокировать.',
      );

    case VaultState.opened:
      return _opened(status);
  }
}

VaultMessage _opened(VaultStatus status) {
  if (!status.hardwareBacked) {
    // R-002 explicitly requires: where there is no hardware keystore — an
    // honest warning, not a silent fallback to a weak scheme.
    return VaultMessage(
      tone: VaultTone.warning,
      title: 'Ключ не в железе',
      detail:
          'Система сообщает: ${levelLabel(status)}. Значит, ключ хранилища '
          'лежит не в защищённом модуле, а в обычной памяти устройства. '
          'Снятый образ хранилища в этом случае поддаётся вскрытию. '
          'Приложение работает, но защита слабее обещанной.',
      action: '',
    );
  }

  return VaultMessage(
    tone: VaultTone.good,
    title: 'Ключ в защищённом модуле',
    detail:
        'Система сообщает: ${levelLabel(status)}. Ключ не выгружается '
        'наружу, поэтому скопированный каталог данных и резервная копия '
        'бесполезны. Телефон, отобранный разблокированным, это не '
        'закрывает — закроет пин.',
    action: '',
  );
}

/// Whether "start over" may be offered.
///
/// Exactly one state gives the right to it. Offering to erase everything on a
/// transient firmware failure means destroying the owner's messages on their
/// behalf.
bool mayOfferFreshStart(VaultStatus status) =>
    status.state == VaultState.keyGone;

/// Whether to show a warning next to the normal screen.
bool needsHonestWarning(VaultStatus status) =>
    describeVault(status).tone != VaultTone.good;
