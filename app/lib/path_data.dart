/// Атрибут `d` как числа: разбор, преобразования, обратная запись.
///
/// Зачем отдельно от `svg_path.dart`: Android принимает **тот же синтаксис**
/// в `android:pathData`, поэтому иконку запуска не нужно растеризовать —
/// достаточно повторить те же преобразования и выписать координаты обратно.
/// Для этого путь нужен списком чисел, а не непрозрачным `Path`, и без
/// `dart:ui`: генератор иконки запускается обычным `dart run`, вне Flutter.
///
/// `svg_path.dart` построен поверх этого файла, так что разбор один на всех:
/// то, что рисует приложение, и то, что уходит в иконку, читается из одной
/// строки одним кодом. Разойтись они не могут.
///
/// Поддержаны команды `M m L l H h V v C c Z z` — этого хватает для вывода
/// potrace и Inkscape. Встретив неподдержанную (`S Q T A`), разбор бросает
/// [FormatException] с указанием команды, а не рисует молча неправильное.
library;

import 'dart:math' as math;

/// Сегмент пути в абсолютных координатах.
sealed class PathSeg {
  const PathSeg();
}

final class MoveSeg extends PathSeg {
  const MoveSeg(this.x, this.y);
  final double x, y;
}

final class LineSeg extends PathSeg {
  const LineSeg(this.x, this.y);
  final double x, y;
}

final class CubicSeg extends PathSeg {
  const CubicSeg(this.x1, this.y1, this.x2, this.y2, this.x, this.y);
  final double x1, y1, x2, y2, x, y;
}

final class CloseSeg extends PathSeg {
  const CloseSeg();
}

final _token = RegExp(
  r'[MmLlHhVvCcSsQqTtAaZz]|[-+]?(?:\d*\.\d+|\d+\.?)(?:[eE][-+]?\d+)?',
);
final _letter = RegExp(r'^[A-Za-z]$');

/// Разбирает `d` в список сегментов, переводя всё в абсолютные координаты.
List<PathSeg> parsePathData(String d) {
  final t = _token.allMatches(d).map((m) => m[0]!).toList();
  final out = <PathSeg>[];

  var i = 0;
  double cx = 0, cy = 0; // текущая точка
  double sx = 0, sy = 0; // начало подпути, куда возвращает Z
  var cmd = '';

  double n() {
    if (i >= t.length) {
      throw const FormatException('путь оборван: не хватает координат');
    }
    return double.parse(t[i++]);
  }

  while (i < t.length) {
    if (_letter.hasMatch(t[i])) cmd = t[i++];
    if (i >= t.length && cmd != 'Z' && cmd != 'z') break;

    switch (cmd) {
      case 'M':
        cx = n();
        cy = n();
        out.add(MoveSeg(cx, cy));
        sx = cx;
        sy = cy;
        cmd = 'L'; // повтор координат после M означает линии
      case 'm':
        cx += n();
        cy += n();
        out.add(MoveSeg(cx, cy));
        sx = cx;
        sy = cy;
        cmd = 'l';
      case 'L':
        cx = n();
        cy = n();
        out.add(LineSeg(cx, cy));
      case 'l':
        cx += n();
        cy += n();
        out.add(LineSeg(cx, cy));
      case 'H':
        cx = n();
        out.add(LineSeg(cx, cy));
      case 'h':
        cx += n();
        out.add(LineSeg(cx, cy));
      case 'V':
        cy = n();
        out.add(LineSeg(cx, cy));
      case 'v':
        cy += n();
        out.add(LineSeg(cx, cy));
      case 'C':
        final x1 = n(), y1 = n(), x2 = n(), y2 = n();
        cx = n();
        cy = n();
        out.add(CubicSeg(x1, y1, x2, y2, cx, cy));
      case 'c':
        // Все шесть чисел отсчитываются от точки НА НАЧАЛО команды,
        // поэтому cx/cy обновляются последними.
        final x1 = cx + n(), y1 = cy + n();
        final x2 = cx + n(), y2 = cy + n();
        final ex = cx + n(), ey = cy + n();
        out.add(CubicSeg(x1, y1, x2, y2, ex, ey));
        cx = ex;
        cy = ey;
      case 'Z':
      case 'z':
        out.add(const CloseSeg());
        cx = sx;
        cy = sy;
      default:
        throw FormatException('команда пути «$cmd» не поддержана');
    }
  }
  return out;
}

/// Прямоугольник без `dart:ui`.
class Box {
  const Box(this.left, this.top, this.right, this.bottom);

  /// Квадрат со стороной [side] с центром в ([cx], [cy]).
  factory Box.square(double cx, double cy, double side) =>
      Box(cx - side / 2, cy - side / 2, cx + side / 2, cy + side / 2);

  final double left, top, right, bottom;

  double get width => right - left;
  double get height => bottom - top;
  double get centerX => (left + right) / 2;
  double get centerY => (top + bottom) / 2;
  bool get isEmpty => width <= 0 || height <= 0;

  Box deflate(double d) => Box(left + d, top + d, right - d, bottom - d);

  @override
  String toString() => 'Box($left, $top, $right, $bottom)';
}

/// Границы пути **по всем точкам, включая контрольные**.
///
/// Именно так считает `Path.getBounds()` в `dart:ui` (проверено тестом:
/// у кубики `M0,0 C0,100 100,100 100,0` высота выходит 100, а не 75).
/// Совпадение обязательно: иначе иконка и то, что рисует приложение,
/// разъедутся по масштабу — ровно на это налетел прошлый подход.
Box boundsOfPathData(List<PathSeg> segs) {
  var l = double.infinity, t = double.infinity;
  var r = double.negativeInfinity, b = double.negativeInfinity;
  var seen = false;

  void point(double x, double y) {
    seen = true;
    if (x < l) l = x;
    if (x > r) r = x;
    if (y < t) t = y;
    if (y > b) b = y;
  }

  for (final s in segs) {
    switch (s) {
      case MoveSeg(:final x, :final y):
        point(x, y);
      case LineSeg(:final x, :final y):
        point(x, y);
      case CubicSeg(
        :final x1,
        :final y1,
        :final x2,
        :final y2,
        :final x,
        :final y,
      ):
        point(x1, y1);
        point(x2, y2);
        point(x, y);
      case CloseSeg():
        break;
    }
  }
  return seen ? Box(l, t, r, b) : const Box(0, 0, 0, 0);
}

/// Плоское аффинное преобразование в порядке SVG `matrix(a b c d e f)`:
/// `x' = a·x + c·y + e`, `y' = b·x + d·y + f`.
class Aff {
  const Aff(this.a, this.b, this.c, this.d, this.e, this.f);

  const Aff.scale(double sx, double sy) : this(sx, 0, 0, sy, 0, 0);
  const Aff.translate(double tx, double ty) : this(1, 0, 0, 1, tx, ty);

  /// Поворот вокруг точки ([cx], [cy]).
  ///
  /// Ось Y направлена вниз, поэтому **положительный угол вращает по часовой
  /// стрелке**, а отрицательный — против.
  factory Aff.rotationAbout(double degrees, double cx, double cy) {
    final r = degrees * math.pi / 180;
    final cos = math.cos(r), sin = math.sin(r);
    return Aff(
      cos,
      sin,
      -sin,
      cos,
      cx - (cx * cos - cy * sin),
      cy - (cx * sin + cy * cos),
    );
  }

  static const identity = Aff(1, 0, 0, 1, 0, 0);

  final double a, b, c, d, e, f;

  /// Сначала `this`, затем [next].
  Aff then(Aff next) => Aff(
    next.a * a + next.c * b,
    next.b * a + next.d * b,
    next.a * c + next.c * d,
    next.b * c + next.d * d,
    next.a * e + next.c * f + next.e,
    next.b * e + next.d * f + next.f,
  );

  double mapX(double x, double y) => a * x + c * y + e;
  double mapY(double x, double y) => b * x + d * y + f;
}

List<PathSeg> transformPathData(List<PathSeg> segs, Aff m) => [
  for (final s in segs)
    switch (s) {
      MoveSeg(:final x, :final y) => MoveSeg(m.mapX(x, y), m.mapY(x, y)),
      LineSeg(:final x, :final y) => LineSeg(m.mapX(x, y), m.mapY(x, y)),
      CubicSeg(
        :final x1,
        :final y1,
        :final x2,
        :final y2,
        :final x,
        :final y,
      ) =>
        CubicSeg(
          m.mapX(x1, y1),
          m.mapY(x1, y1),
          m.mapX(x2, y2),
          m.mapY(x2, y2),
          m.mapX(x, y),
          m.mapY(x, y),
        ),
      CloseSeg() => const CloseSeg(),
    },
];

/// Отражает путь по горизонтали относительно собственных границ.
/// Двойник `mirrorPathX` из `svg_path.dart`.
List<PathSeg> mirrorDataX(List<PathSeg> segs) {
  final b = boundsOfPathData(segs);
  return transformPathData(segs, Aff(-1, 0, 0, 1, b.left + b.right, 0));
}

/// Поворачивает путь вокруг центра его границ.
/// Двойник `rotatePath` из `svg_path.dart`.
List<PathSeg> rotateData(List<PathSeg> segs, double degrees) {
  if (degrees == 0) return segs;
  final b = boundsOfPathData(segs);
  return transformPathData(
    segs,
    Aff.rotationAbout(degrees, b.centerX, b.centerY),
  );
}

/// Вписывает путь в прямоугольник, сохраняя пропорции.
/// Двойник `fitPath` из `svg_path.dart`.
List<PathSeg> fitData(List<PathSeg> segs, Box box, {double inset = 0}) {
  final b = boundsOfPathData(segs);
  if (b.isEmpty) return segs;

  final target = box.deflate(inset);
  if (target.isEmpty) return segs;

  final k = (target.width / b.width) < (target.height / b.height)
      ? target.width / b.width
      : target.height / b.height;

  final w = b.width * k, h = b.height * k;
  final dx = target.left + (target.width - w) / 2;
  final dy = target.top + (target.height - h) / 2;

  return transformPathData(
    segs,
    Aff(k, 0, 0, k, dx - k * b.left, dy - k * b.top),
  );
}

/// Записывает сегменты обратно в атрибут `d`.
///
/// Буква команды опускается там, где повторяется — так делает любой SVG-экспорт,
/// и `PathParser` в Android это понимает. [precision] знаков после запятой;
/// в поле 108 единиц двух хватает с запасом: 0,01 единицы — это 0,04 пикселя
/// на самой крупной плотности.
String formatPathData(List<PathSeg> segs, {int precision = 2}) {
  final buf = StringBuffer();
  var last = '';

  void cmd(String letter, List<double> nums) {
    if (letter != last) {
      if (buf.isNotEmpty) buf.write(' ');
      buf.write(letter);
      last = letter;
    } else {
      buf.write(' ');
    }
    for (var i = 0; i < nums.length; i++) {
      buf.write(i == 0 ? '' : (i.isEven ? ' ' : ','));
      buf.write(_num(nums[i], precision));
    }
  }

  for (final s in segs) {
    switch (s) {
      case MoveSeg(:final x, :final y):
        cmd('M', [x, y]);
      case LineSeg(:final x, :final y):
        cmd('L', [x, y]);
      case CubicSeg(
        :final x1,
        :final y1,
        :final x2,
        :final y2,
        :final x,
        :final y,
      ):
        cmd('C', [x1, y1, x2, y2, x, y]);
      case CloseSeg():
        cmd('Z', const []);
        last = ''; // после Z следующая команда пишется буквой
    }
  }
  return buf.toString();
}

String _num(double v, int precision) {
  var s = v.toStringAsFixed(precision);
  if (s.contains('.')) {
    s = s.replaceFirst(RegExp(r'0+$'), '');
    s = s.replaceFirst(RegExp(r'\.$'), '');
  }
  return s == '-0' ? '0' : s;
}
