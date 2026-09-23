/// Разбор атрибута `d` из SVG в [Path].
///
/// Зачем своё, а не пакет: нужна одна функция под один фирменный контур.
/// Тянуть ради этого зависимость с её обновлениями и уязвимостями — плохой
/// размен в проекте, который обещает проверяемость. Здесь сотня строк,
/// которые можно прочитать целиком.
///
/// Поддержаны команды `M m L l H h V v C c Z z` — этого хватает для вывода
/// potrace и Inkscape. Встретив неподдержанную (`S Q T A`), функция бросает
/// [FormatException] с указанием команды, а не рисует молча неправильно.
library;

import 'dart:math' as math;
import 'dart:typed_data';
import 'dart:ui';

final _token = RegExp(
  r'[MmLlHhVvCcSsQqTtAaZz]|[-+]?(?:\d*\.\d+|\d+\.?)(?:[eE][-+]?\d+)?',
);
final _letter = RegExp(r'^[A-Za-z]$');

Path parseSvgPath(String d) {
  final t = _token.allMatches(d).map((m) => m[0]!).toList();
  final path = Path();

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
        path.moveTo(cx, cy);
        sx = cx;
        sy = cy;
        cmd = 'L'; // повтор координат после M означает линии
      case 'm':
        cx += n();
        cy += n();
        path.moveTo(cx, cy);
        sx = cx;
        sy = cy;
        cmd = 'l';
      case 'L':
        cx = n();
        cy = n();
        path.lineTo(cx, cy);
      case 'l':
        cx += n();
        cy += n();
        path.lineTo(cx, cy);
      case 'H':
        cx = n();
        path.lineTo(cx, cy);
      case 'h':
        cx += n();
        path.lineTo(cx, cy);
      case 'V':
        cy = n();
        path.lineTo(cx, cy);
      case 'v':
        cy += n();
        path.lineTo(cx, cy);
      case 'C':
        final x1 = n(), y1 = n(), x2 = n(), y2 = n();
        cx = n();
        cy = n();
        path.cubicTo(x1, y1, x2, y2, cx, cy);
      case 'c':
        // Все шесть чисел отсчитываются от точки НА НАЧАЛО команды,
        // поэтому cx/cy обновляются последними.
        final x1 = cx + n(), y1 = cy + n();
        final x2 = cx + n(), y2 = cy + n();
        final ex = cx + n(), ey = cy + n();
        path.cubicTo(x1, y1, x2, y2, ex, ey);
        cx = ex;
        cy = ey;
      case 'Z':
      case 'z':
        path.close();
        cx = sx;
        cy = sy;
      default:
        throw FormatException('команда пути «$cmd» не поддержана');
    }
  }
  return path;
}

/// Плоское аффинное преобразование в виде, который принимает [Path.transform]:
/// матрица 4×4 по столбцам.
///
/// Собирается вручную, чтобы утилита обходилась одним `dart:ui` и не тянула
/// vector_math ради масштаба и сдвига.
Float64List affine({
  double scaleX = 1,
  double scaleY = 1,
  double translateX = 0,
  double translateY = 0,
}) {
  final m = Float64List(16);
  m[0] = scaleX;
  m[5] = scaleY;
  m[10] = 1;
  m[12] = translateX;
  m[13] = translateY;
  m[15] = 1;
  return m;
}

/// Отражает путь по горизонтали относительно собственных границ.
Path mirrorPathX(Path source) {
  final b = source.getBounds();
  return source.transform(
    affine(scaleX: -1, translateX: b.left + b.right),
  );
}

/// Поворачивает путь вокруг центра его границ.
///
/// Ось Y экрана направлена вниз, поэтому **положительный угол вращает по
/// часовой стрелке**, а отрицательный — против. Применять поворот следует
/// после отражения: зеркало меняет направление вращения на обратное.
Path rotatePath(Path source, double degrees) {
  if (degrees == 0) return source;
  final c = source.getBounds().center;
  final r = degrees * math.pi / 180;
  final cos = math.cos(r), sin = math.sin(r);

  final m = Float64List(16);
  m[0] = cos;
  m[1] = sin;
  m[4] = -sin;
  m[5] = cos;
  m[10] = 1;
  m[15] = 1;
  // Сдвиг, возвращающий центр на место после поворота вокруг начала координат.
  m[12] = c.dx - (c.dx * cos - c.dy * sin);
  m[13] = c.dy - (c.dx * sin + c.dy * cos);
  return source.transform(m);
}

/// Вписывает путь в прямоугольник, сохраняя пропорции.
///
/// [mirrorX] отражает по горизонтали: исходный силуэт летит влево, а в
/// интерфейсе с письмом слева направо отправка читается движением вправо.
Path fitPath(
  Path source,
  Rect box, {
  bool mirrorX = false,
  double inset = 0,
}) {
  final b = source.getBounds();
  if (b.isEmpty) return source;

  final target = box.deflate(inset);
  if (target.isEmpty) return source;

  final k = (target.width / b.width) < (target.height / b.height)
      ? target.width / b.width
      : target.height / b.height;

  final w = b.width * k, h = b.height * k;
  final dx = target.left + (target.width - w) / 2;
  final dy = target.top + (target.height - h) / 2;

  return source.transform(
    affine(
      scaleX: mirrorX ? -k : k,
      scaleY: k,
      translateX: mirrorX ? dx + w + k * b.left : dx - k * b.left,
      translateY: dy - k * b.top,
    ),
  );
}
