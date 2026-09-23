/// Сборка ресурсов иконки запуска Android из того же контура, что и в приложении.
///
/// Здесь только чистые функции, возвращающие содержимое файлов; запись —
/// в `gen_android_icon.dart`, проверка — в `test/android_icon_test.dart`.
/// Разделение нужно ровно для того, чтобы тест мог сверить лежащие в репозитории
/// ресурсы с тем, что генератор выдал бы сейчас.
///
/// **Растеризации нет.** Android принимает тот же синтаксис `pathData`, что и
/// SVG, поэтому иконка — это те же координаты, пересчитанные один раз.
/// Прошлые два подхода (генератор на `flutter_test`, headless-браузер) ломались
/// именно на растеризации и масштабе; здесь ломаться нечему.
library;

import 'package:apeiron/brand/mark_geometry.dart';
import 'package:apeiron/brand/raven_path.dart';
import 'package:apeiron/path_data.dart';

/// Поле адаптивной иконки — 108 dp.
const double adaptiveViewport = 108;

/// Из поля система показывает центральные 72 dp: под круглой маской —
/// круг того же диаметра. Это и есть подложка, роль которой в приложении
/// играет круг `ApeironAppIcon`.
const double adaptiveMask = 72;

/// Цвета марки в записи Android.
///
/// Дублируют `Ap.basalt900` и `Ap.bone100` из `theme/tokens.dart`: тот файл
/// тянет Flutter, а генератор работает без него. Расхождение ловит тест
/// `android_icon_test.dart` — дублирование здесь объявленное, не случайное.
const String basaltHex = '#FF12161A';
const String boneHex = '#FFE8E6E1';

/// Ворон в том же положении, в каком его рисует `ApeironRaven`:
/// флип вывода potrace, затем зеркало, затем разворот. Порядок обязателен —
/// зеркало меняет направление вращения на обратное.
List<PathSeg> ravenShape() {
  var s = parsePathData(ravenPathData);
  s = transformPathData(s, const Aff.scale(1, ravenSourceFlipY));
  if (ravenFacesRight) s = mirrorDataX(s);
  return rotateData(s, ravenDefaultPitch);
}

/// Квадрат, в который вписана птица на подложке диаметра [shellDiameter].
///
/// То же, что делает `ApeironAppIcon`: он кладёт `ApeironRaven` размером
/// `iconGlyphScale` от подложки по её центру.
Box glyphBox(double shellDiameter) => Box.square(
  adaptiveViewport / 2,
  adaptiveViewport / 2,
  iconGlyphScale * shellDiameter,
);

/// Ворон, вписанный в подложку диаметра [shellDiameter] по центру поля
/// [adaptiveViewport], — ровно как `ApeironAppIcon` вписывает его в круг.
List<PathSeg> ravenFittedToShell(double shellDiameter) =>
    fitData(ravenShape(), glyphBox(shellDiameter));

/// Круг подложки на всё поле — для устройств до Android 8, где маски нет
/// и рисовать её приходится самому.
List<PathSeg> shellCircle() {
  const r = adaptiveViewport / 2;
  const c = adaptiveViewport / 2;
  // Четверть окружности приближается кубикой с плечом k·r; k = 0,5523 —
  // классическое значение, ошибка меньше 0,02 % радиуса.
  const k = 0.5522847498307936 * r;
  return const [
    MoveSeg(c, 0),
    CubicSeg(c + k, 0, adaptiveViewport, c - k, adaptiveViewport, c),
    CubicSeg(
      adaptiveViewport,
      c + k,
      c + k,
      adaptiveViewport,
      c,
      adaptiveViewport,
    ),
    CubicSeg(c - k, adaptiveViewport, 0, c + k, 0, c),
    CubicSeg(0, c - k, c - k, 0, c, 0),
    CloseSeg(),
  ];
}

const String _warning =
    '<!-- СГЕНЕРИРОВАНО из assets/brand/raven-corvus-corax-cc0.svg — руками не править.\n'
    '     Пересобрать: cd app && dart run tool/gen_android_icon.dart -->';

String _vector(String body) =>
    '''<?xml version="1.0" encoding="utf-8"?>
$_warning
<vector xmlns:android="http://schemas.android.com/apk/res/android"
    android:width="${_dp(adaptiveViewport)}"
    android:height="${_dp(adaptiveViewport)}"
    android:viewportWidth="${_plain(adaptiveViewport)}"
    android:viewportHeight="${_plain(adaptiveViewport)}">
$body
</vector>
''';

String _path(List<PathSeg> segs, String fill) =>
    '    <path\n'
    '        android:fillColor="$fill"\n'
    '        android:pathData="${formatPathData(segs)}" />';

/// Передний слой адаптивной иконки: одна птица на прозрачном поле.
/// Подложку рисует система из цвета, маску накладывает тоже она.
String foregroundXml() =>
    _vector(_path(ravenFittedToShell(adaptiveMask), boneHex));

/// Монохромный слой (Android 13 и новее, «тематические иконки»):
/// та же геометрия, цвет система заменит своим.
String monochromeXml() =>
    _vector(_path(ravenFittedToShell(adaptiveMask), '#FFFFFFFF'));

/// Иконка целиком для Android 7, где адаптивных иконок ещё нет:
/// круг подложки и птица на нём.
String legacyXml() => _vector(
  '${_path(shellCircle(), basaltHex)}\n'
  '${_path(ravenFittedToShell(adaptiveViewport), boneHex)}',
);

String adaptiveXml() =>
    '''<?xml version="1.0" encoding="utf-8"?>
$_warning
<adaptive-icon xmlns:android="http://schemas.android.com/apk/res/android">
    <background android:drawable="@color/ic_launcher_background" />
    <foreground android:drawable="@drawable/ic_launcher_foreground" />
    <monochrome android:drawable="@drawable/ic_launcher_monochrome" />
</adaptive-icon>
''';

String backgroundColorXml() =>
    '''<?xml version="1.0" encoding="utf-8"?>
$_warning
<resources>
    <color name="ic_launcher_background">$basaltHex</color>
</resources>
''';

/// Заставка запуска: тот же басальт. Белая заставка Flutter по умолчанию
/// вспыхивает перед тёмным приложением — это видно и это дефект.
String launchBackgroundXml() =>
    '''<?xml version="1.0" encoding="utf-8"?>
$_warning
<layer-list xmlns:android="http://schemas.android.com/apk/res/android">
    <item android:drawable="@color/ic_launcher_background" />
</layer-list>
''';

/// Что и куда кладём. Путь — от каталога `app/`.
Map<String, String> androidIconFiles() => {
  'android/app/src/main/res/values/ic_launcher_background.xml':
      backgroundColorXml(),
  'android/app/src/main/res/drawable/ic_launcher_foreground.xml':
      foregroundXml(),
  'android/app/src/main/res/drawable/ic_launcher_monochrome.xml':
      monochromeXml(),
  'android/app/src/main/res/mipmap-anydpi-v26/ic_launcher.xml': adaptiveXml(),
  // Без `-v26`: квалификатор `anydpi` понимают с Android 5, а нижняя
  // граница проекта — Android 7. Плотностные PNG не нужны вовсе.
  'android/app/src/main/res/mipmap-anydpi/ic_launcher.xml': legacyXml(),
  'android/app/src/main/res/drawable/launch_background.xml':
      launchBackgroundXml(),
  'android/app/src/main/res/drawable-v21/launch_background.xml':
      launchBackgroundXml(),
};

String _dp(double v) => '${_plain(v)}dp';

String _plain(double v) =>
    v == v.roundToDouble() ? v.round().toString() : v.toString();
