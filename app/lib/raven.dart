import 'package:flutter/material.dart';

import 'brand/mark_geometry.dart';
import 'brand/raven_path.dart';
import 'svg_path.dart';
import 'theme/tokens.dart';

/// The Apeiron icon: a raven in flight.
///
/// Huginn and Muninn are Odin's ravens, who fly around the world and come back
/// to tell what they saw. Messengers in the literal sense; nothing in the Norse
/// set of images is closer to a messenger app. The names translate as
/// **"thought"** and **"memory"**, which matches the layering of the system:
/// delivery handles "now", the archive handles what is kept.
///
/// **The silhouette is not drawn here but taken ready-made.** Six iterations of
/// hand-picking Bézier control points produced a bird, but not a realistic one:
/// blindly typing coordinates has a low ceiling. The source is CC0 from
/// PhyloPic; provenance and rationale in `assets/brand/PROVENANCE.md`.
///
/// **Curves are allowed here.** This is an exception to the system rule: the
/// wordmark ([ApeironWordmark]) and the interface icons stay on straight lines
/// with flat ends — runic carving — while the bird is alive.
class ApeironRaven extends StatelessWidget {
  const ApeironRaven({
    super.key,
    this.size = 64,
    this.color = Ap.bone100,
    this.facingRight = ravenFacesRight,
    this.pitchDegrees = defaultPitch,
    this.inset = 0,
  });

  /// Default rotation — rationale at [ravenDefaultPitch].
  static const double defaultPitch = ravenDefaultPitch;

  final double size;
  final Color color;

  /// The source silhouette flies left. In a left-to-right interface, sending
  /// reads as movement to the right, so we mirror by default.
  final bool facingRight;

  /// Rotation of the silhouette in degrees, applied after mirroring.
  final double pitchDegrees;

  /// Inset from the edges of the field, in logical pixels.
  final double inset;

  @override
  Widget build(BuildContext context) {
    return SizedBox.square(
      dimension: size,
      child: CustomPaint(
        painter: _RavenPainter(color, facingRight, pitchDegrees, inset),
      ),
    );
  }
}

class _RavenPainter extends CustomPainter {
  const _RavenPainter(this.color, this.facingRight, this.pitch, this.inset);

  final Color color;
  final bool facingRight;
  final double pitch;
  final double inset;

  /// The outline is parsed once per process: parsing a four-thousand-character
  /// string on every repaint is a pointless waste.
  static final Path _source = _buildSource();

  static Path _buildSource() {
    final raw = parseSvgPath(ravenPathData);
    // potrace output is flipped vertically — put it back.
    return raw.transform(affine(scaleY: ravenSourceFlipY));
  }

  @override
  void paint(Canvas canvas, Size size) {
    final p = Paint()
      ..color = color
      ..style = PaintingStyle.fill
      ..isAntiAlias = true;
    // The order is mandatory: mirror first, then rotate. The mirror reverses
    // the direction of rotation, and rotating before it sends the beak astray.
    var shape = _source;
    if (facingRight) shape = mirrorPathX(shape);
    shape = rotatePath(shape, pitch);
    canvas.drawPath(fitPath(shape, Offset.zero & size, inset: inset), p);
  }

  @override
  bool shouldRepaint(_RavenPainter old) =>
      old.color != color ||
      old.facingRight != facingRight ||
      old.pitch != pitch ||
      old.inset != inset;
}
