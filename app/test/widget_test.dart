import 'package:apeiron/fingerprint.dart';
import 'package:apeiron/mark.dart';
import 'package:apeiron/theme/tokens.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

/// Тесты системы оформления.
///
/// Экраны здесь не проверяются: `RustLib.init()` требует собранной нативной
/// библиотеки, что делает такой тест интеграционным. Эти проверки стерегут
/// правила из `docs/design.md`, которые легко нарушить не заметив.
void main() {
  group('правило прямых углов', () {
    test('карточки не скруглены', () {
      final shape = Ap.dark().cardTheme.shape;
      expect(shape, isA<RoundedRectangleBorder>());
      expect(
        (shape! as RoundedRectangleBorder).borderRadius,
        BorderRadius.zero,
        reason: 'скругление нарушает единственное жёсткое правило системы',
      );
    });

    test('кнопки не скруглены', () {
      final style = Ap.dark().filledButtonTheme.style;
      final shape = style?.shape?.resolve(<WidgetState>{});
      expect(shape, isA<RoundedRectangleBorder>());
      expect((shape! as RoundedRectangleBorder).borderRadius, BorderRadius.zero);
    });
  });

  group('палитра', () {
    test('основной текст не чистый белый', () {
      // Чистый белый на тёмном режет глаз и выглядит дёшево.
      expect(Ap.bone100, isNot(const Color(0xFFFFFFFF)));
    });

    test('медь зарезервирована и отличима от ледникового', () {
      expect(Ap.ember400, isNot(Ap.glacier400));
    });
  });

  group('моноширинный шрифт', () {
    test('первым идёт JetBrains Mono, подстановки заданы', () {
      // Разборчивость 0/O и 1/l в числе сверки — вопрос безопасности,
      // а не вкуса: путаница означает пропущенного посредника.
      expect(Ap.monoFallback.first, 'JetBrains Mono');
      expect(Ap.monoFallback.length, greaterThan(2));
      expect(Ap.monoFallback.last, 'monospace');
    });

    test('стиль mono() применяет гарнитуру и подстановки', () {
      final s = Ap.mono(size: 26, spacing: 3.4);
      expect(s.fontFamily, 'JetBrains Mono');
      expect(s.fontFamilyFallback, isNotEmpty);
      expect(s.fontSize, 26);
      expect(s.letterSpacing, 3.4);
    });
  });

  group('раскладка отпечатка', () {
    test('шесть групп ложатся в две строки по три', () {
      final rows = fingerprintRows(
          ['30879', '28053', '14932', '68733', '97238', '94718'], 3);
      expect(rows.length, 2);
      expect(rows[0], ['30879', '28053', '14932']);
      expect(rows[1], ['68733', '97238', '94718']);
    });

    test('неполная последняя строка не роняет и не теряет группы', () {
      final rows = fingerprintRows(['a', 'b', 'c', 'd'], 3);
      expect(rows, [
        ['a', 'b', 'c'],
        ['d'],
      ]);
      expect(rows.expand((r) => r).length, 4, reason: 'ни одна группа не потеряна');
    });

    test('пустой вход даёт пустой результат, а не исключение', () {
      expect(fingerprintRows(const [], 3), isEmpty);
    });

    test('per <= 0 не зацикливается', () {
      // Защита от бесконечного цикла: шаг ноль был бы зависанием экрана сверки.
      expect(fingerprintRows(['a', 'b'], 0), [
        ['a', 'b'],
      ]);
      expect(fingerprintRows(['a', 'b'], -3), [
        ['a', 'b'],
      ]);
    });

    test('группы сохраняют порядок при любом разбиении', () {
      final src = List.generate(7, (i) => '$i');
      for (final per in [1, 2, 3, 5, 7, 20]) {
        expect(fingerprintRows(src, per).expand((r) => r).toList(), src,
            reason: 'порядок нарушен при per=$per');
      }
    });
  });

  testWidgets('знак отрисовывается', (tester) async {
    await tester.pumpWidget(
      const MaterialApp(
        home: Scaffold(body: Center(child: ApeironMark(size: 64))),
      ),
    );
    expect(find.byType(ApeironMark), findsOneWidget);
    expect(tester.takeException(), isNull);
  });
}
