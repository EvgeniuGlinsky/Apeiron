/// What to tell a person about the key vault.
///
/// The logic is split out of the widget following [lock_policy.dart] and for
/// the same reason: it is checked by tests, while the screen cannot be checked
/// at all without a built native library. The rule from there carries over
/// literally — **the text must match the behaviour**.
///
/// The wording here is not decoration. The table in `docs/threat-log.md`
/// explicitly forbids promising more than has been done. With the PIN (R-011)
/// a phone taken away unlocked no longer opens the vault — but a guess is still
/// possible on this very phone for someone with root, and extraction of the key
/// from the hardware leaves only the length of the PIN. That is what is said.
library;

import 'src/rust/api/vault.dart' show VaultState, VaultStatus;

/// How serious the state is.
enum VaultTone {
  /// Everything as designed.
  good,

  /// Works, but weaker than promised, or needs a step. Staying silent is not
  /// allowed.
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

/// A wait as people say it: "40 с", "5 мин", "1 ч".
String waitLabel(int seconds) {
  if (seconds < 60) return '$seconds с';
  final minutes = (seconds / 60).ceil();
  if (minutes < 60) return '$minutes мин';
  return '${(minutes / 60).ceil()} ч';
}

const _keyNotExported =
    'Ключ лежал в защищённом модуле этого телефона и наружу не выгружался — '
    'в этом и был смысл, — поэтому достать его неоткуда.';

/// What to show for the current state of the vault.
VaultMessage describeVault(VaultStatus status) {
  switch (status.state) {
    case VaultState.keyGone:
      return const VaultMessage(
        tone: VaultTone.blocked,
        title: 'Ключ этого телефона исчез',
        detail:
            'Ключ хранилища пропал из защищённого модуля. Так бывает после '
            'обновления прошивки, при снятии блокировки экрана и при '
            'восстановлении данных из резервной копии. $_keyNotExported',
        action: 'Начать заново. Прежняя переписка не вернётся.',
      );

    case VaultState.keyMismatch:
      return const VaultMessage(
        tone: VaultTone.blocked,
        title: 'Ключ в защищённом модуле не тот',
        detail:
            'В защищённом модуле лежит ключ, но не тот, которым сделано '
            'хранилище. Это не неверный пин: попытки не тратились. '
            '$_keyNotExported',
        action: 'Начать заново. Прежняя переписка не вернётся.',
      );

    case VaultState.legacyData:
      return const VaultMessage(
        tone: VaultTone.blocked,
        title: 'Данные тестовой сборки до пина',
        detail:
            'Эти данные записала сборка, в которой ещё не было пина. Переносить '
            'их на пин эта сборка не умеет — намеренно: перенос был бы самым '
            'рискованным кодом во всём хранилище ради одного запуска на '
            'тестовых данных. Контактов там нет; новая личность получит новый '
            'отпечаток.',
        action: 'Начать заново с пином.',
      );

    case VaultState.pinSetupRequired:
      return const VaultMessage(
        tone: VaultTone.warning,
        title: 'Задайте пин',
        detail:
            'Без пина хранилище не открыть даже на этом телефоне, даже '
            'разблокированном. Каждую попытку подбора проверяет защищённый '
            'модуль этого телефона, и перенести подбор на другое железо '
            'нельзя. Забытый пин — это потеря всей переписки: восстановления '
            'пока нет.',
        action:
            'От 6 до 16 цифр. 6 цифр держат вора и досмотр; против лаборатории '
            'с root на этом телефоне это часы. 10 цифр — годы. Каждая лишняя '
            'цифра — подбор в 10 раз дольше.',
      );

    case VaultState.pinMismatch:
      return const VaultMessage(
        tone: VaultTone.warning,
        title: 'Пины не совпали',
        detail: 'Второй ввод отличается от первого. Ничего не сохранено.',
        action: 'Задайте пин заново.',
      );

    case VaultState.wrongPin:
      return VaultMessage(
        tone: VaultTone.warning,
        title: 'Неверный пин',
        detail:
            'Ошибок подряд: ${status.failures}. Пять попыток бесплатны, '
            'дальше каждая стоит ожидания: 30 с, 1 мин, 5 мин, 15 мин, потом '
            'по часу.',
        action: status.waitSeconds > 0
            ? 'Следующая попытка через ${waitLabel(status.waitSeconds)}.'
            : 'Попробуйте ещё раз.',
      );

    case VaultState.delayed:
      return VaultMessage(
        tone: VaultTone.warning,
        title: 'Слишком много ошибок',
        detail:
            'Ошибок подряд: ${status.failures}. Пока идёт ожидание, пин не '
            'проверяется вовсе. Перевод часов не помогает, перезагрузка '
            'начинает ожидание заново.',
        action: 'Следующая попытка через ${waitLabel(status.waitSeconds)}.',
      );

    case VaultState.retry:
      return VaultMessage(
        tone: VaultTone.warning,
        title: 'Хранилище сейчас недоступно',
        detail: status.message.isEmpty
            ? 'Защищённый модуль устройства не ответил.'
            : status.message,
        action: 'Данные целы, попытка не потрачена. Повторите.',
      );

    case VaultState.unavailable:
      return const VaultMessage(
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
            'Ключ хранилища затёрт в памяти. Собрать его заново можно только '
            'из пина, и каждую попытку проверяет защищённый модуль этого '
            'телефона.',
        action: 'Введите пин.',
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
          'Система сообщает: ${levelLabel(status)}. Значит, ключ пина лежит '
          'не в защищённом модуле, а в обычной памяти устройства, и подбор '
          'пина не упирается в железо. Приложение работает, но защита '
          'слабее обещанной.',
      action: '',
    );
  }

  return VaultMessage(
    tone: VaultTone.good,
    title: 'Хранилище под пином',
    detail:
        'Система сообщает: ${levelLabel(status)}. Скопированный каталог, '
        'резервная копия и телефон, отобранный разблокированным, без пина '
        'бесполезны. Подбирать пин можно только на этом телефоне, по одной '
        'проверке железом за попытку. С root на этом телефоне 6 цифр — от '
        'часов до дней, 8 — от недель до года, 10 — годы; оценка для этого '
        'телефона — в самопроверке. Если ключ извлекут из железа, держит '
        'только длина пина.',
    action: status.unlockMs > 0
        ? 'Разблокировка заняла ${status.unlockMs} мс.'
        : '',
  );
}

/// Whether "start over" may be offered.
///
/// Only when the key is really lost. Offering to erase everything on a
/// transient firmware failure or a wrong PIN means destroying the owner's
/// messages on their behalf.
bool mayOfferFreshStart(VaultStatus status) =>
    status.state == VaultState.keyGone ||
    status.state == VaultState.keyMismatch;

/// Whether this is the state of a test build's data from before the PIN.
bool isLegacy(VaultStatus status) => status.state == VaultState.legacyData;

/// Whether the PIN pad is what the person should see now.
bool needsPin(VaultStatus status) => const {
  VaultState.locked,
  VaultState.wrongPin,
  VaultState.delayed,
}.contains(status.state);

/// Whether a new PIN has to be set.
bool needsPinSetup(VaultStatus status) =>
    status.state == VaultState.pinSetupRequired ||
    status.state == VaultState.pinMismatch;

/// Whether to show a warning next to the normal screen.
bool needsHonestWarning(VaultStatus status) =>
    describeVault(status).tone != VaultTone.good;
