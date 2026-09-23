import 'package:flutter/widgets.dart';

/// Когда личность запирается сама.
///
/// Решение R-001 («пин при каждом возврате») писалось про телефон: самый частый
/// сценарий принуждения — аппарат, отобранный разблокированным. На телефоне
/// уход приложения с переднего плана и есть тот момент, когда защита должна
/// сработать, и заодно он совпадает с гашением экрана: система сама переводит
/// приложение в фон, отдельный таймер бездействия там не нужен.
///
/// **На рабочем столе это правило вредит.** Там потеря фокуса — это щелчок
/// в браузер, а не выпускание устройства из рук; запирание на каждом
/// переключении окна превращает защиту в помеху, а помеху отключают. Поэтому
/// на десктопе фокус не считается событием, но появляется то, чего на телефоне
/// нет: **таймер бездействия**. Эквивалент «телефон отобрали» здесь —
/// «отошёл от компьютера», и измеряется он именно так.
///
/// Логика вынесена из виджета отдельно, чтобы её можно было проверить без
/// запуска приложения: цена ошибки здесь — молча незапертая личность.
@immutable
class LockPolicy {
  const LockPolicy({required this.locksOnFocusLoss, required this.idleTimeout});

  /// Политика для платформы, на которой идёт работа.
  factory LockPolicy.of(TargetPlatform platform) => switch (platform) {
    TargetPlatform.android || TargetPlatform.iOS || TargetPlatform.fuchsia =>
      const LockPolicy(locksOnFocusLoss: true, idleTimeout: null),
    TargetPlatform.windows ||
    TargetPlatform.macOS ||
    TargetPlatform.linux => const LockPolicy(
      locksOnFocusLoss: false,
      // Три минуты: меньше — человек не успевает прочитать длинное сообщение
      // и вернуться к клавиатуре, больше — за это время до открытого экрана
      // успевает дойти кто угодно.
      idleTimeout: Duration(minutes: 3),
    ),
  };

  /// Запирать ли при `inactive` — потере фокуса без ухода с экрана.
  final bool locksOnFocusLoss;

  /// Через сколько бездействия запирать. `null` — не по времени.
  final Duration? idleTimeout;

  /// Надо ли запереться при переходе в [state].
  bool locksOn(AppLifecycleState state) => switch (state) {
    AppLifecycleState.resumed => false,
    // Приложение ещё на экране, но ввод уходит другому окну.
    AppLifecycleState.inactive => locksOnFocusLoss,
    // Окно свёрнуто, перекрыто или приложение уходит совсем — везде запираем.
    AppLifecycleState.hidden ||
    AppLifecycleState.paused ||
    AppLifecycleState.detached => true,
  };

  /// Как объяснить это пользователю. Обещание в интерфейсе должно совпадать
  /// с тем, что код делает на **этой** платформе, а не вообще.
  String get explanation {
    if (locksOnFocusLoss) {
      return 'При уходе приложения в фон личность уничтожается вместе '
          'с ключами.';
    }
    final minutes = idleTimeout?.inMinutes;
    return 'Личность уничтожается вместе с ключами, когда окно свёрнуто'
        '${minutes == null ? '' : ' или $minutes минуты нет действий'}. '
        'Переключение на другое окно её не трогает: на рабочем столе это '
        'происходит слишком часто, чтобы что-то значить.';
  }

  @override
  bool operator ==(Object other) =>
      other is LockPolicy &&
      other.locksOnFocusLoss == locksOnFocusLoss &&
      other.idleTimeout == idleTimeout;

  @override
  int get hashCode => Object.hash(locksOnFocusLoss, idleTimeout);
}
