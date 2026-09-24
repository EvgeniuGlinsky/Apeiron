/// Brand mark numbers shared by the app and the launcher icon generator.
///
/// The file is deliberately **free of `dart:ui` and Flutter**: it is read both
/// by the widget and by `tool/gen_android_icon.dart`, which runs under plain
/// `dart run`. As long as these numbers live in one place, the launcher icon
/// and the bird in the UI cannot drift apart — and they would drift unnoticed.
library;

/// Default rotation of the silhouette, in degrees.
///
/// The source holds the beak slightly below the horizon; a raised beak reads
/// as climbing rather than descending. A negative angle is counter-clockwise
/// (the screen's Y axis points down). Chosen from a contact sheet of the
/// series 0 / −20 / −32 / −45.
const double ravenDefaultPitch = -45;

/// The source silhouette flies left. In a left-to-right interface, sending
/// reads as movement to the right, so we mirror by default.
const bool ravenFacesRight = true;

/// Share of the backplate taken by the bird.
///
/// Below 0.55 the icon looks empty; above 0.7 the silhouette runs into the
/// edges and loses its outline.
const double iconGlyphScale = 0.70;
