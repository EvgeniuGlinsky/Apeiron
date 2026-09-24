/// Builds the Android launcher icon resources from the same outline as the app.
///
/// Only pure functions returning file contents live here; writing is in
/// `gen_android_icon.dart`, checking is in `test/android_icon_test.dart`.
/// The split exists precisely so that the test can compare the resources in the
/// repository against what the generator would produce now.
///
/// **There is no rasterisation.** Android accepts the same `pathData` syntax as
/// SVG, so the icon is the same coordinates, recomputed once.
/// The previous two approaches (a `flutter_test` generator, a headless browser)
/// broke precisely on rasterisation and scale; here there is nothing to break.
library;

import 'package:apeiron/brand/mark_geometry.dart';
import 'package:apeiron/brand/raven_path.dart';
import 'package:apeiron/path_data.dart';

/// The adaptive icon field is 108 dp.
const double adaptiveViewport = 108;

/// Of the field the system shows the central 72 dp: under a round mask, a
/// circle of that diameter. This is the backplate, whose role in the app is
/// played by the `ApeironAppIcon` circle.
const double adaptiveMask = 72;

/// Brand mark colours in Android notation.
///
/// They duplicate `Ap.basalt900` and `Ap.bone100` from `theme/tokens.dart`:
/// that file pulls in Flutter, and the generator works without it. A mismatch
/// is caught by the `android_icon_test.dart` test — the duplication here is
/// declared, not accidental.
const String basaltHex = '#FF12161A';
const String boneHex = '#FFE8E6E1';

/// The raven in the same position `ApeironRaven` draws it in:
/// the potrace output flip, then the mirror, then the rotation. The order is
/// mandatory — the mirror reverses the direction of rotation.
List<PathSeg> ravenShape() {
  var s = parsePathData(ravenPathData);
  s = transformPathData(s, const Aff.scale(1, ravenSourceFlipY));
  if (ravenFacesRight) s = mirrorDataX(s);
  return rotateData(s, ravenDefaultPitch);
}

/// The square the bird is fitted into on a backplate of diameter
/// [shellDiameter].
///
/// Same as what `ApeironAppIcon` does: it places `ApeironRaven` at
/// `iconGlyphScale` of the backplate, at its centre.
Box glyphBox(double shellDiameter) => Box.square(
  adaptiveViewport / 2,
  adaptiveViewport / 2,
  iconGlyphScale * shellDiameter,
);

/// The raven fitted into a backplate of diameter [shellDiameter] at the centre
/// of the [adaptiveViewport] field — exactly as `ApeironAppIcon` fits it into
/// the circle.
List<PathSeg> ravenFittedToShell(double shellDiameter) =>
    fitData(ravenShape(), glyphBox(shellDiameter));

/// The backplate circle over the whole field — for devices before Android 8,
/// where there is no mask and it has to be drawn by hand.
List<PathSeg> shellCircle() {
  const r = adaptiveViewport / 2;
  const c = adaptiveViewport / 2;
  // A quarter circle is approximated by a cubic with handle k·r; k = 0.5523 is
  // the classic value, error under 0.02 % of the radius.
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
    '<!-- GENERATED from assets/brand/raven-corvus-corax-cc0.svg — do not edit by hand.\n'
    '     Regenerate: cd app && dart run tool/gen_android_icon.dart -->';

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

/// Foreground layer of the adaptive icon: a single bird on a transparent field.
/// The system draws the backplate from the colour, and applies the mask too.
String foregroundXml() =>
    _vector(_path(ravenFittedToShell(adaptiveMask), boneHex));

/// Monochrome layer (Android 13 and newer, "themed icons"):
/// the same geometry; the system replaces the colour with its own.
String monochromeXml() =>
    _vector(_path(ravenFittedToShell(adaptiveMask), '#FFFFFFFF'));

/// The whole icon for Android 7, where there are no adaptive icons yet:
/// the backplate circle and the bird on it.
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

/// Launch screen: the same basalt. Flutter's default white launch screen
/// flashes before the dark app — it is visible, and it is a defect.
String launchBackgroundXml() =>
    '''<?xml version="1.0" encoding="utf-8"?>
$_warning
<layer-list xmlns:android="http://schemas.android.com/apk/res/android">
    <item android:drawable="@color/ic_launcher_background" />
</layer-list>
''';

/// What goes where. Paths are relative to the `app/` directory.
Map<String, String> androidIconFiles() => {
  'android/app/src/main/res/values/ic_launcher_background.xml':
      backgroundColorXml(),
  'android/app/src/main/res/drawable/ic_launcher_foreground.xml':
      foregroundXml(),
  'android/app/src/main/res/drawable/ic_launcher_monochrome.xml':
      monochromeXml(),
  'android/app/src/main/res/mipmap-anydpi-v26/ic_launcher.xml': adaptiveXml(),
  // No `-v26`: the `anydpi` qualifier is understood since Android 5, and the
  // project's lower bound is Android 7. Density-specific PNGs are not needed.
  'android/app/src/main/res/mipmap-anydpi/ic_launcher.xml': legacyXml(),
  'android/app/src/main/res/drawable/launch_background.xml':
      launchBackgroundXml(),
  'android/app/src/main/res/drawable-v21/launch_background.xml':
      launchBackgroundXml(),
};

String _dp(double v) => '${_plain(v)}dp';

String _plain(double v) =>
    v == v.roundToDouble() ? v.round().toString() : v.toString();
