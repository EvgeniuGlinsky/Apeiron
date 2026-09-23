/// Разбор атрибута `d` из SVG в [Path] и преобразования над готовым путём.
///
/// Зачем своё, а не пакет: нужна одна функция под один фирменный контур.
/// Тянуть ради этого зависимость с её обновлениями и уязвимостями — плохой
/// размен в проекте, который обещает проверяемость. Здесь сотня строк,
/// которые можно прочитать целиком.
///
/// Разбор чисел живёт в `path_data.dart` — без `dart:ui`, потому что тем же
/// разбором пользуется генератор иконки Android (`tool/gen_android_icon.dart`),
/// запускаемый вне Flutter. Здесь остаётся мост в [Path] и те же
/// преобразования, но над непрозрачным путём.
library;

import 'dart:math' as math;
import 'dart:typed_data';
import 'dart:ui';

import 'path_data.dart';

/// Разбирает атрибут `d`. Неподдержанная команда (`S Q T A`) бросает
/// [FormatException] с её указанием, а не рисует молча неправильное.
Path parseSvgPath(String d) => buildPath(parsePathData(d));

/// Собирает [Path] из разобранных сегментов.
Path buildPath(List<PathSeg> segs) {
  final path = Path();
  for (final s in segs) {
    switch (s) {
      case MoveSeg(:final x, :final y):
        path.moveTo(x, y);
      case LineSeg(:final x, :final y):
        path.lineTo(x, y);
      case CubicSeg(
        :final x1,
        :final y1,
        :final x2,
        :final y2,
        :final x,
        :final y,
      ):
        path.cubicTo(x1, y1, x2, y2, x, y);
      case CloseSeg():
        path.close();
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
  return source.transform(affine(scaleX: -1, translateX: b.left + b.right));
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
Path fitPath(Path source, Rect box, {bool mirrorX = false, double inset = 0}) {
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
