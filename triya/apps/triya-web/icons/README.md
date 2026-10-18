# Triya CAD 3D app icons

These PNGs are exports of the Triya mark, which is drawn in code (`triya/crates/triya-brand/src/mark.rs`:
a vertical pressure vessel with 2:1 heads, a flanged nozzle and a skirt on a slate tile). Don't
edit them by hand: change the drawing and run

    TRIYA_WRITE_ICONS=1 cargo test -p triya-ui --test icons

The same test (without the variable) fails when a file here differs from the drawing, and checks
that `../manifest.webmanifest` and `../index.html` list exactly these files. Each file has a row in
the root `ATTRIBUTION.md`.

| file | size | manifest `purpose` |
|---|---|---|
| `32x32/in.openengtools.triya.png` | 32 | any (favicon) |
| `192x192/in.openengtools.triya.png` | 192 | any (Chrome's install minimum) |
| `256x256/in.openengtools.triya.png` | 256 | any (apple-touch-icon) |
| `512x512/in.openengtools.triya.png` | 512 | any (splash screen, og:image) |
| `512x512/in.openengtools.triya-maskable.png` | 512 | maskable (full-bleed tile, drawing inside the 80 % safe zone) |

Upstream SolidCraft's icons are SolidCraft's branding and are not copied into the fork.
