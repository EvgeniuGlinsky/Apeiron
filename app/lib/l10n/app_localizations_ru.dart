// ignore: unused_import
import 'package:intl/intl.dart' as intl;
import 'app_localizations.dart';

// ignore_for_file: type=lint

/// The translations for Russian (`ru`).
class AppLocalizationsRu extends AppLocalizations {
  AppLocalizationsRu([String locale = 'ru']) : super(locale);

  @override
  String get selfCheckTooltip => 'Самопроверка';

  @override
  String get lockTooltip => 'Заблокировать';

  @override
  String get noIdentityTitle => 'ЛИЧНОСТИ ЕЩЁ НЕТ';

  @override
  String get noIdentityBody =>
      'В хранилище этого устройства личности ещё нет. Вместе с ней будут заведены ключи устройства и журнал личности — порознь они бессмысленны.';

  @override
  String get createIdentity => 'СОЗДАТЬ ЛИЧНОСТЬ';

  @override
  String get fingerprintTitle => 'ВАШ ОТПЕЧАТОК';

  @override
  String get fingerprintBody =>
      'По нему узнают вас. С собеседником сверяют не его: откройте чат → «Сверить», там число сверки — одно и то же у вас обоих.';

  @override
  String get notVerified => 'НЕ СВЕРЕНО НИ С КЕМ';

  @override
  String get keySigning => 'ED25519 · ПОДПИСЬ';

  @override
  String get keyAgreement => 'X25519 · СОГЛАСОВАНИЕ';

  @override
  String get copy => 'Скопировать';

  @override
  String get copied => 'Скопировано';

  @override
  String get trueNowTitle => 'ЧТО ЗДЕСЬ УЖЕ ПРАВДА';

  @override
  String trueNowBody(String explanation) {
    return 'Секретные ключи не покидают Rust — сюда пришли только публичные половины и отпечаток. $explanation';
  }

  @override
  String get notYetTitle => 'КАК ИДУТ СООБЩЕНИЯ';

  @override
  String get notYetBody =>
      'Сообщения идут через Mainline DHT — сеть, которой пользуется BitTorrent: у неё нет владельца, и нашего сервера на пути нет. Содержимое защищено; кто с кем говорит — сети видно.';

  @override
  String get lockExplainMobile =>
      'При уходе приложения в фон личность уничтожается вместе с ключами.';

  @override
  String lockExplainDesktop(int minutes) {
    String _temp0 = intl.Intl.pluralLogic(
      minutes,
      locale: localeName,
      other: '$minutes минуты',
      many: '$minutes минут',
      few: '$minutes минуты',
      one: '$minutes минуту',
    );
    return 'Личность уничтожается вместе с ключами, когда окно свёрнуто или $_temp0 нет действий. Переключение на другое окно её не трогает: на рабочем столе это происходит слишком часто, чтобы что-то значить.';
  }

  @override
  String get lockExplainDesktopNoTimeout =>
      'Личность уничтожается вместе с ключами, когда окно свёрнуто. Переключение на другое окно её не трогает: на рабочем столе это происходит слишком часто, чтобы что-то значить.';

  @override
  String get keyNotExported =>
      'Ключ лежал в защищённом модуле этого телефона и наружу не выгружался — в этом и был смысл, — поэтому достать его неоткуда.';

  @override
  String get startOverAction => 'Начать заново. Прежняя переписка не вернётся.';

  @override
  String get vaultKeyGoneTitle => 'Ключ этого телефона исчез';

  @override
  String get vaultKeyGoneDetail =>
      'Ключ хранилища пропал из защищённого модуля. Так бывает после обновления прошивки, при снятии блокировки экрана и при восстановлении данных из резервной копии.';

  @override
  String get vaultKeyMismatchTitle => 'Ключ в защищённом модуле не тот';

  @override
  String get vaultKeyMismatchDetail =>
      'В защищённом модуле лежит ключ, но не тот, которым сделано хранилище. Это не неверный пин: попытки не тратились.';

  @override
  String get vaultLegacyTitle => 'Данные тестовой сборки до пина';

  @override
  String get vaultLegacyDetail =>
      'Эти данные записала сборка, в которой ещё не было пина. Переносить их на пин эта сборка не умеет — намеренно: перенос был бы самым рискованным кодом во всём хранилище ради одного запуска на тестовых данных. Контактов там нет; новая личность получит новый отпечаток.';

  @override
  String get vaultLegacyAction => 'Начать заново с пином.';

  @override
  String get vaultPinSetupTitle => 'Задайте пин';

  @override
  String get vaultPinSetupDetail =>
      'Без пина хранилище не открыть даже на этом телефоне, даже разблокированном. Каждую попытку подбора проверяет защищённый модуль этого телефона, и перенести подбор на другое железо нельзя. Забытый пин — это потеря всей переписки: восстановления пока нет.';

  @override
  String get vaultPinSetupAction =>
      'От 6 до 16 цифр. 6 цифр держат вора и досмотр; против лаборатории с root на этом телефоне это часы. 10 цифр — годы. Каждая лишняя цифра — подбор в 10 раз дольше.';

  @override
  String get vaultPinMismatchTitle => 'Пины не совпали';

  @override
  String get vaultPinMismatchDetail =>
      'Второй ввод отличается от первого. Ничего не сохранено.';

  @override
  String get vaultPinMismatchAction => 'Задайте пин заново.';

  @override
  String get vaultWrongPinTitle => 'Неверный пин';

  @override
  String vaultWrongPinDetail(int failures) {
    return 'Ошибок подряд: $failures. Пять попыток бесплатны, дальше каждая стоит ожидания: 30 с, 1 мин, 5 мин, 15 мин, потом по часу.';
  }

  @override
  String get tryAgain => 'Попробуйте ещё раз.';

  @override
  String nextAttemptIn(String wait) {
    return 'Следующая попытка через $wait.';
  }

  @override
  String get vaultDelayedTitle => 'Слишком много ошибок';

  @override
  String vaultDelayedDetail(int failures) {
    return 'Ошибок подряд: $failures. Пока идёт ожидание, пин не проверяется вовсе. Перевод часов не помогает, перезагрузка начинает ожидание заново.';
  }

  @override
  String get vaultRetryTitle => 'Хранилище сейчас недоступно';

  @override
  String get vaultRetryFallback => 'Защищённый модуль устройства не ответил.';

  @override
  String get vaultRetryAction =>
      'Данные целы, попытка не потрачена. Повторите.';

  @override
  String get vaultUnavailableTitle => 'На этой платформе хранилища нет';

  @override
  String get vaultUnavailableDetail =>
      'Аппаратного хранилища ключей здесь не существует, а класть ключ в файл рядом с данными и называть это защитой — неправда. Сборка для этой платформы заморожена.';

  @override
  String get vaultLockedTitle => 'Заперто';

  @override
  String get vaultLockedDetail =>
      'Ключ хранилища затёрт в памяти. Собрать его заново можно только из пина, и каждую попытку проверяет защищённый модуль этого телефона.';

  @override
  String get vaultLockedAction => 'Введите пин.';

  @override
  String get vaultSoftwareTitle => 'Ключ не в железе';

  @override
  String vaultSoftwareDetail(String level) {
    return 'Система сообщает: $level. Значит, ключ пина лежит не в защищённом модуле, а в обычной памяти устройства, и подбор пина не упирается в железо. Приложение работает, но защита слабее обещанной.';
  }

  @override
  String get vaultOpenedTitle => 'Хранилище под пином';

  @override
  String vaultOpenedDetail(String level) {
    return 'Система сообщает: $level. Скопированный каталог, резервная копия и телефон, отобранный разблокированным, без пина бесполезны. Подбирать пин можно только на этом телефоне, по одной проверке железом за попытку. С root на этом телефоне 6 цифр — от часов до дней, 8 — от недель до года, 10 — годы; оценка для этого телефона — в самопроверке. Если ключ извлекут из железа, держит только длина пина.';
  }

  @override
  String vaultUnlockTook(int ms) {
    return 'Разблокировка заняла $ms мс.';
  }

  @override
  String get levelSoftware => 'программный';

  @override
  String get levelUnknownSecure => 'железо без уточнения';

  @override
  String get levelUnknown => 'неизвестно';

  @override
  String waitSeconds(int n) {
    return '$n с';
  }

  @override
  String waitMinutes(int n) {
    return '$n мин';
  }

  @override
  String waitHours(int n) {
    return '$n ч';
  }

  @override
  String get pinRepeat => 'ПОВТОРИТЕ ПИН';

  @override
  String get pinNew => 'НОВЫЙ ПИН';

  @override
  String get pinEnter => 'ВВЕДИТЕ ПИН';

  @override
  String get pinLayoutNote =>
      'Раскладка новая на каждую попытку: смотреть на палец бесполезно, на экран — нет. Экран закрыт от снимков и записи.';

  @override
  String get pinErase => 'Стереть';

  @override
  String get pinDone => 'Готово';

  @override
  String get retry => 'ПОВТОРИТЬ';

  @override
  String get startOver => 'НАЧАТЬ ЗАНОВО';

  @override
  String get startOverWithPin => 'НАЧАТЬ ЗАНОВО С ПИНОМ';

  @override
  String get selfCheckTitle => 'САМОПРОВЕРКА';

  @override
  String get copyReport => 'Скопировать отчёт целиком';

  @override
  String get reportCopied => 'Отчёт скопирован';

  @override
  String get runAgain => 'Прогнать заново';

  @override
  String get allPassed => 'Всё прошло.';

  @override
  String failedCount(int failed, int total) {
    return 'Не прошло: $failed из $total.';
  }

  @override
  String get checkPassed => '[ да ]';

  @override
  String get checkFailed => '[ НЕТ]';

  @override
  String get dhtTitle => 'ЗАМЕР DHT';

  @override
  String get dhtBody =>
      'Можно ли доставлять сообщения без единого сервера. Конверты тестовые: случайные байты, ничего о вас. Положите, через несколько часов проверьте свои, и заберите конверты, которые положил ПК. Журнал сохраняется между запусками.';

  @override
  String dhtPut(int count) {
    return 'ПОЛОЖИТЬ $count';
  }

  @override
  String get dhtCheckOwn => 'ПРОВЕРИТЬ СВОИ';

  @override
  String get dhtFetchDesktop => 'ЗАБРАТЬ С ПК';

  @override
  String get dhtClearLog => 'ОЧИСТИТЬ ЖУРНАЛ';

  @override
  String get platformTitle => 'ПЛАТФОРМА';

  @override
  String get platformBody =>
      'Секретов здесь нет. Это то, что можно переслать целиком.';

  @override
  String get pinChoiceTitle => 'КАКИМ БУДЕТ ПИН';

  @override
  String get pinChoiceLength => 'ДЛИНА';

  @override
  String pinDigits(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count цифры',
      many: '$count цифр',
      few: '$count цифры',
      one: '$count цифра',
    );
    return '$_temp0';
  }

  @override
  String get pinChoiceKeyboard => 'КЛАВИАТУРА';

  @override
  String get pinKeyboardScrambled => 'Перемешанная';

  @override
  String get pinKeyboardOrdered => 'Обычная';

  @override
  String get pinRecommendedTag => 'рекомендуем';

  @override
  String get pinChoiceRecommend =>
      'Рекомендуем 8 цифр и перемешанную клавиатуру: это самый надёжный вариант.';

  @override
  String get pinChoiceHonest =>
      'Тот, кто заберёт этот телефон и получит на нём root, может перебирать пины в его защищённом чипе: 4 цифры падают за минуты, 6 — примерно за полсуток, 8 — примерно за месяц. Перемешанная клавиатура скрывает пин от того, кто видит только ваш палец.';

  @override
  String get pinChoiceContinue => 'ДАЛЕЕ';

  @override
  String get pinOrderedNote => 'Экран закрыт от снимков и записи.';

  @override
  String get settingsTooltip => 'Настройки';

  @override
  String get settingsTitle => 'НАСТРОЙКИ';

  @override
  String get settingsPin => 'ПИН';

  @override
  String get settingsKeyboard => 'Перемешанная клавиатура';

  @override
  String get settingsKeyboardNote =>
      'Новая раскладка на каждую попытку. Рекомендуем.';

  @override
  String get settingsChangePin => 'Сменить пин';

  @override
  String get settingsChangePinNote =>
      'Ничего не перешифровывается: под новый пин запечатывается только ключ данных.';

  @override
  String get settingsIdentity => 'МОЯ ЛИЧНОСТЬ';

  @override
  String get settingsSelfCheck => 'Самопроверка';

  @override
  String get changePinTitle => 'СМЕНА ПИНА';

  @override
  String get pinCurrent => 'ТЕКУЩИЙ ПИН';

  @override
  String get pinChanged => 'Пин сменён';

  @override
  String get chatsEmptyTitle => 'ПОКА НИ ОДНОЙ ПЕРЕПИСКИ';

  @override
  String get chatsEmptyBody =>
      'Пригласите кого-нибудь или примите приглашение, которое прислали вам. Сообщения идут через DHT BitTorrent: ни один наш сервер их не хранит.';

  @override
  String get invite => 'ПРИГЛАСИТЬ';

  @override
  String get acceptInvitation => 'ПРИНЯТЬ ПРИГЛАШЕНИЕ';

  @override
  String get invitationsWaiting => 'ПРИГЛАШЕНИЯ ЖДУТ ОТВЕТА';

  @override
  String invitationUntil(String date) {
    return 'до $date';
  }

  @override
  String get contactWaiting => 'ждёт, пока примут';

  @override
  String get contactNotAccepted => 'приглашение не принято';

  @override
  String get contactTaken => 'на приглашение первым ответил кто-то другой';

  @override
  String get contactDamaged => 'переписка повреждена';

  @override
  String get contactVerified => 'сверено';

  @override
  String get contactNotVerified => 'не сверено';

  @override
  String get inviteTitle => 'ПРИГЛАШЕНИЕ';

  @override
  String get inviteName => 'Для кого (имя, которое вы увидите)';

  @override
  String get inviteCreate => 'СОЗДАТЬ ПРИГЛАШЕНИЕ';

  @override
  String get inviteBody =>
      'Отправьте этот текст любым путём. Кто прочтёт его по дороге, сможет ответить от имени этого человека, поэтому потом сверьте с ним число сверки — лично или голосом.';

  @override
  String inviteExpires(String date) {
    return 'Действует до $date.';
  }

  @override
  String get inviteWithdraw => 'ОТОЗВАТЬ';

  @override
  String get acceptPaste => 'Текст приглашения (apeiron:…)';

  @override
  String get acceptName => 'Имя для контакта';

  @override
  String get acceptFirst => 'Первое сообщение (необязательно)';

  @override
  String get acceptButton => 'ПРИНЯТЬ';

  @override
  String get acceptNote =>
      'Контакт будет ждать, пока пригласивший не заберёт ваш ответ.';

  @override
  String get messageHint => 'Сообщение';

  @override
  String get send => 'Отправить';

  @override
  String get msgQueued => 'ещё не отправлено';

  @override
  String get msgSent => 'отправлено';

  @override
  String get msgDelivered => 'доставлено';

  @override
  String get msgNotDelivered => 'не доставлено';

  @override
  String get msgAddressTaken => 'не доставлено: адрес занят';

  @override
  String get msgLost => 'Часть их сообщений потеряна';

  @override
  String get chatWaitingNote =>
      'Приглашение ещё не принято. Писать можно: сообщения подождут.';

  @override
  String get chatClosedNote => 'Эта переписка больше ничего не принесёт.';

  @override
  String get chatEmpty => 'Сообщений пока нет.';

  @override
  String get verifyAction => 'Сверить';

  @override
  String get verifyTitle => 'ЧИСЛО СВЕРКИ';

  @override
  String verifyBody(String name) {
    return 'Одно число на двоих: $name видит на этом экране ровно эти цифры. Это не ваш отпечаток из настроек.';
  }

  @override
  String get verifyConfirm => 'ЧИСЛА СОВПАЛИ';

  @override
  String get verifyUndo => 'СНЯТЬ ОТМЕТКУ';

  @override
  String get verifiedByYou => 'СВЕРЕНО ВАМИ';

  @override
  String verifyStep1(String name) {
    return 'Позвоните $name — голосом, не через тот канал, по которому шло приглашение.';
  }

  @override
  String verifyStep2(String name) {
    return 'Откройте этот экран оба. Вы читаете вслух первую строку, $name — вторую.';
  }

  @override
  String get verifyStep3 =>
      'Все цифры одинаковы — совпало. Отличается хоть одна — не совпало.';

  @override
  String get verifyMismatch => 'НЕ СОВПАЛО';

  @override
  String get mismatchTitle => 'ЧИСЛА НЕ СОВПАЛИ';

  @override
  String get mismatchBody =>
      'Пока это не выяснено, не пишите ничего, что не сказали бы постороннему: между вами может быть кто-то. Узнайте, чей ключ не тот, что у другого, — сравните два отпечатка ниже с тем, что каждый из вас видит в «Настройки → Моя личность».';

  @override
  String mismatchTheirs(String name) {
    return 'Отпечаток $name, как он записан у вас. У $name в настройках должен быть такой же:';
  }

  @override
  String mismatchMine(String name) {
    return 'Ваш отпечаток. У $name он должен быть записан таким же:';
  }

  @override
  String get mismatchVerdict =>
      'Отпечаток отличается — ключ подменён: этой переписке не доверять. Оба совпадают, а числа всё равно разные — это ошибка приложения: пришлите снимки обоих экранов.';

  @override
  String get mismatchBack => 'К ЧИСЛУ СВЕРКИ';

  @override
  String get msgRead => 'прочитано';

  @override
  String previewMine(String text) {
    return 'Вы: $text';
  }

  @override
  String get settingsReadReceipts => 'Отметки о прочтении';

  @override
  String get settingsReadReceiptsNote =>
      'Собеседник узнаёт, когда вам показали его сообщение, а вы — то же о своих. Выключено — ни то, ни другое не отправляется и не показывается. Сети это нового почти ничего не говорит: открытый чат и так виден по частоте запросов.';
}
