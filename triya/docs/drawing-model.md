# Drawing model and the vessel GA generator

`triya-drawing` turns a resolved vessel (+ measured weights) into a **sheet**, renders it through a
`Plotter`, and exports SVG/DXF/PDF (ADR-0004). Units: mm on the sheet; views carry a scale. The
kickoff skeleton already has the model types, the `Plotter` trait and an `SvgPlotter`.

## 1. Data model (`triya-drawing::model`)
```
Sheet { size: A1|A2|A3|A4|Custom{w,h}, landscape, template: TitleBlockTemplate,
        title_block: BTreeMap<String,String>, revisions: Vec<Revision>,
        views: Vec<View>, tables: Vec<Table>, notes: Vec<String> }
View  { id, kind: Elevation|Plan|Section{plane}|Detail{of, scale}|Isometric(post-beta),
        scale: f64, origin: Pt (sheet mm), rotation,
        edges: Vec<Edge>, dims: Vec<Dimension>, labels: Vec<Label>, hatch: Vec<HatchRegion>,
        center_lines: Vec<Edge> }
Edge  { geom: Line{a,b}|Arc{c,r,a0,a1}|Circle{c,r}|Ellipse{c,rx,ry,rot,a0,a1}|Polyline(Vec<Pt>),
        layer: Outline|Hidden|Center|Phantom|Hatch|Dim|Text|Title, width: Thin|Thick }
Dimension { kind: Linear{a,b,offset,dir}|Diameter{circle,angle}|Radius{arc}|Angular{c,a0,a1,r}|Ordinate{origin,pts},
            text_override: Option<String>, source: FieldId }        // FieldId = "V101.E3.length"
Label { anchor: Pt, text, leader: Option<Pt>, style: Note|Balloon{n}|Mark{id} }
HatchRegion { loop_: Vec<Edge>, pattern: Ansi31{spacing, angle} }
Table { id, origin, kind: DesignData|NozzleSchedule|Bom|Notes|Revisions|Weights, columns, rows }
```
Every view edge and dimension that comes from the model records its `source` field id so a
regenerate re-derives it instead of re-detecting geometry.

## 2. Vessel views (analytic; `vessel_views.rs`)
**Elevation** (vertical vessel: axis vertical on the sheet; horizontal vessel: axis horizontal):
- Shell course: two silhouette lines at ±OD/2; **tangent lines** (thin, extended beyond the
  outline) at each head TL; circumferential seams as thin lines where course lengths change.
- Heads: outside meridian profile (SF line, knuckle arc, crown arc) as `Arc`s — exact, so DXF
  keeps arcs; inside profile only in the section view.
- Cones: two straight lines, knuckle arcs when toriconical.
- Nozzles: neck silhouettes along the projected frame axis (hidden where inside the shell
  silhouette: the inside-radius test marks that part `Hidden`), flange as a rectangle (OD ×
  thickness) with the raised face as a thinner rectangle; pads as a rectangle on the shell. Hillside
  nozzles appear at their true projected offset. Nozzle mark label with leader.
- Supports: skirt (lines + base ring), saddles (web, base plate rectangles), legs (projected
  rectangles), brackets, lugs; rings as thin rectangles; limpet coil as a hatched band with pitch
  note (not every turn).
- Projection rule: elevation looks along −Y (plan north at top); a second elevation at 90° is
  generated when any nozzle lies within ±15° of the viewing direction.

**Plan**: outer circle (OD), skirt/base ring circles or saddle/leg footprint, nozzle stubs as a
radial line + small circle at the flange OD at each azimuth, azimuth angular dimensions from the
`angle_zero` reference, North arrow.

**Section A–A** (through the axis, at the azimuth of the most nozzles): the inside and outside
profile loops of the whole stack (the revolve profile is the section — no 3D cut needed), nozzle
necks that lie in the plane as cut rectangles, 45° hatch (ANSI 31) clipped to the metal region by
even-odd scanline clipping of the line family against the loop polygons.

**Details** (post-beta): nozzle weld details, pad details, saddle details from a template library.

## 3. Rule-based dimensioning (`dims.rs`)
Driven from the resolved stations; stacked in rows at fixed offsets from the outline with a
collision nudge (shift text by one row if two texts overlap).
1. Row 1 (closest): course lengths and head depths (TL to seam, seam to seam).
2. Row 2: nozzle and attachment centrelines chained from the **datum TL** (`axial` resolved to
   datum regardless of `axial.ref`), each labelled with its mark.
3. Row 3: TL–TL, overall length/height incl. skirt or saddle base, base-ring elevation.
4. Horizontal row (plan/elevation): nozzle projections from vessel CL to flange face
   (`projection.from` honoured in the text, e.g. "1100 FROM CL"), saddle spacing and distance
   from TL, skirt OD.
5. Notes instead of dimensions: ID/OD + thickness per course ("1600 ID × 12 THK"), head type,
   SF, crown/knuckle radii, cone half-apex angle.
6. Plan: azimuths as angular dimensions; hillside offsets as linear dims.
Text style: ISO 3098-like sans at 3.5 mm (5 mm titles) on the sheet; all texts upright.

## 4. Tables
| table | columns |
|---|---|
| Design data | Code · Design P int/ext · Design T · MAWP · MDMT · Hydrotest · CA · Joint eff · RT · PWHT · Wind/Seismic · Capacity · Weights (empty/operating/test) · Serial/Year |
| Nozzle schedule | Mark · Qty · Size (NPS) · Sch/Thk · Rating · Facing · Type · Elevation (from datum) · Azimuth · Projection · Service · Remarks (pad, hillside offset) |
| BOM | Item · Qty · Description (e.g. "SHELL COURSE 1600 ID × 12 THK × 3000 LG") · Material · Std/Ref (`standardRef` text) · Unit weight · Total weight |
| Weights | computed metal weight per element, attachments, totals; "Handbook check" column |
| Notes | numbered general notes (template defaults + `notes[]`) |
| Revisions | Rev · Date · By · Description |

## 5. Templates
SVG files with named placeholders (`{{title}}`, `{{drawing_no}}`, `{{rev}}`, `{{scale}}`,
`{{sheet}}`, `{{org}}`, `{{logo}}`), one per sheet size; the OET default template is original
work (ATTRIBUTION row — every `.svg` in the tree needs one). Users may upload their own template
(OPFS) — placeholders documented.

## 6. Plotter trait and backends (`plot.rs`)
```
trait Plotter { fn begin(&mut self, sheet: &SheetSize); fn set_layer(&mut self, Layer); fn set_width(&mut self, f64);
                fn set_style(&mut self, LineStyle /* Solid|Dashed|Center|Phantom */);
                fn line(&mut self, Pt, Pt); fn arc(&mut self, Pt, f64, f64, f64); fn circle(&mut self, Pt, f64);
                fn ellipse(&mut self, …); fn polyline(&mut self, &[Pt]); fn text(&mut self, Pt, &str, TextStyle);
                fn hatch_lines(&mut self, &[(Pt, Pt)]); fn end(&mut self) -> Result<Vec<u8>, PlotError>; }
```
- **SVG** (own writer; skeleton's `SvgPlotter`): layers → `<g id="OUTLINE">`; dashes via
  `stroke-dasharray`; text as `<text>`. The egui preview does **not** parse SVG: it paints
  `Edge`s directly with `epaint` shapes; SVG is only the export.
- **DXF**: first target upstream's public writer `solidcraft_io::write_dxf_drawing` with
  `DxfLayer/DxfEntity/DxfShape` (`crates/io/src/dxf_out.rs`, already used by
  `file.export_view_dxf` / `sheet.export_dxf`); add the `dxf` crate (0.6.x, MIT) only for
  entities it lacks (TEXT/MTEXT, dashed linetypes, ELLIPSE) — decide in T6.6. Layers
  `OUTLINE, HIDDEN (DASHED), CENTER, DIM, TEXT, HATCH, TITLE`; beta dimensions **exploded** into
  lines, arrowheads and TEXT; hatch as LINEs on `HATCH`. Post-beta: DIMENSION entities with `*D` blocks.
- **PDF** (decided in T6.6): Triya's own PDF 1.4 writer (`triya-drawing/src/pdf.rs`), not
  `printpdf`: one page the size of the sheet, vector lines and Bézier arcs, filled arrowheads,
  texts in base-14 Helvetica with WinAnsi encoding (nothing embedded, so wasm needs no font
  files; characters outside WinAnsi print `?`). DXF uses upstream's writer alone (no `dxf`
  crate): layers, hidden/dashed linetypes, ARC/CIRCLE/ELLIPSE/TEXT are all there.

## 7. Golden tests
`tests/golden/<case>.svg|dxf|pdf` compared byte-for-byte after normalising ids/timestamps; a
`--update-golden` env var regenerates. Cases: `rect_dim_table`, `vessel_a_elevation`,
`vessel_a_plan`, `vessel_a_section`, `vessel_b_horizontal`, `vessel_c_legs`, and the GA
sheets `vessel_{a,b,c}_ga` (vessel A also as DXF and PDF; `TRIYA_UPDATE_GOLDEN=1` rewrites). Golden `.svg` files
need ATTRIBUTION rows ("generated test fixture") or `cargo xtask assets` fails.

## 8. Commands
`drawing.generate_ga {size, views, tables, scale, template, title_block, sheet?}` → stores the
*options* in `Document.metadata["in.openengtools.triya.sheet"]` (a whole sheet exceeds the 64 KiB
value cap and would go stale; the sheet is rebuilt from the vessel each time) and returns a
summary (`sheet: true` adds the sheet) · `drawing.export {format: svg|dxf|pdf|json, path?}` →
a file on desktop, a download for `/downloads/<name>` in the browser, else the text (base64 for
PDF, the sheet model for json). There is no `drawing.preview` command (programmatic calls never
open dialogs): the preview is the Vessel panel's Drawing page.