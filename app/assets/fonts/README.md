# Шрифты

Сюда кладутся файлы гарнитур. Все три — под SIL Open Font License, бандлить в приложение
разрешено, менять и распространять тоже.

## Почему не `google_fonts`

Пакет `google_fonts` по умолчанию **скачивает шрифты с fonts.gstatic.com при первом запуске**.
Для мессенджера, обещающего отсутствие обращений наружу, это сообщает Google факт установки
приложения — с IP-адреса пользователя, до того как он что-либо сделал. Недопустимо. Шрифты
только отсюда, из assets.

## Что нужно положить

| Файл | Гарнитура | Начертание | Где применяется |
|---|---|---|---|
| `Inter-Regular.ttf` | Inter | 400 | основной текст |
| `Inter-Medium.ttf` | Inter | 500 | подписи |
| `Inter-SemiBold.ttf` | Inter | 600 | заголовки интерфейса |
| `Syne-SemiBold.ttf` | Syne | 600 | крупные заголовки |
| `Syne-Bold.ttf` | Syne | 700 | логотип, титулы |
| `JetBrainsMono-Regular.ttf` | JetBrains Mono | 400 | ключи, коды |
| `JetBrainsMono-SemiBold.ttf` | JetBrains Mono | 600 | **отпечаток на экране сверки** |

Годятся и переменные версии (`Inter-VariableFont.ttf` и аналоги) — тогда файлов будет меньше,
но объявление в `pubspec.yaml` придётся поправить.

## Где взять

- Inter — https://rsms.me/inter/ либо https://fonts.google.com/specimen/Inter
- Syne — https://fonts.google.com/specimen/Syne
- JetBrains Mono — https://www.jetbrains.com/lp/mono/

Скачивать архивом с сайта, не через пакет.

## Почему именно JetBrains Mono для отпечатка

Он однозначно различает `0` и `O`, `1` и `l` и `I`. На экране сверки это не вопрос вкуса:
спутанная цифра означает, что пользователь принял чужой ключ за ключ собеседника — то есть
пропустил посредника, против которого вся эта криптография и строится.

## После добавления файлов

1. Объявить их в `app/pubspec.yaml` в разделе `flutter: fonts:`.
2. В `lib/theme/tokens.dart` заменить `uiFont = null` на `'Inter'`, `displayFont = null`
   на `'Syne'`.
3. `flutter pub get` и пересобрать.
