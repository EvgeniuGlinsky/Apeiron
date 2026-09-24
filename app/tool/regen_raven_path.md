# How to regenerate `lib/brand/raven_path.dart`

The file is generated from `assets/brand/raven-corvus-corax-cc0.svg` and must not be edited by hand:
any manual edit will be lost on the next regeneration and, worse, will diverge from the source
that `PROVENANCE.md` refers to.

## What is inside the source SVG

It is potrace output, and it is always structured the same way:

```xml
<svg width="1247.000000pt" height="1451.000000pt" viewBox="0 0 1247.000000 1451.000000">
  <g transform="translate(0.000000,1451.000000) scale(0.100000,-0.100000)" fill="#000000">
    <path d="M6274 14437 c-118 -73 ..."/>
  </g>
</svg>
```

Two things matter:

1. **the `d` attribute of the single `<path>`** — it becomes the `ravenPathData` constant;
2. **the sign of the Y scale in the group `transform`** — potrace makes it negative, and without
   flipping it back the bird flies upside down. This is the `ravenSourceFlipY` constant.

Everything else in the `transform` (the translation and the scale magnitude) is not needed:
`fitPath` fits the outline into the field and recomputes them itself. Sizes are in **points**, not
pixels — this is exactly what tripped up the headless-browser approach, which got the wrong scale.

## Procedure

1. Put the new SVG into `assets/brand/`, update `PROVENANCE.md` (source, author, licence,
   date checked). Verify the licence on the page of the image itself, not of the collection.
2. Check that the path consists only of the commands `M m L l H h V v C c Z z`. The parser
   (`lib/path_data.dart`) throws `FormatException` on the others rather than silently drawing
   something wrong. Inkscape can simplify down to cubics: "Path → Simplify", then save as
   "Plain SVG".
3. Move `d` into `ravenPathData` and the sign of the Y scale into `ravenSourceFlipY`; leave the
   file header as is.
4. Regenerate the launcher icon: `dart run tool/gen_android_icon.dart`.
5. Run `flutter test` and look with your own eyes:
   `flutter test test/raven_sheet.dart` and `flutter test test/android_icon_sheet.dart`
   (these two processes do not exit — wait for the PNGs to appear in `build/mark/`, not for the exit).
