# SDK reference examples

These are proposed API shapes from [RUN-03/SDK-01](../decisions/ADR-RUN-03.md). None of them compiles yet. CI gate `SDK-examples-7` must compile each example in Rust and TypeScript against the fake host, and run it on both apps where the capability is advertised. Helpers belong to the SDK; `cx.ui.prompt`, `cx.overlay.publish`, one-shot `cx.query` and `cx.commit` are defined as [SDK sugar](sdk.md#sdk-sugar). Large state lives in guest memory, and `cx.state` is limited to 64 KiB.

```ts
// 1 PANEL
export default panel({ id: "stats", site: "right" }, {
  async mount(cx, m) { await draw(cx, m); },
  async event(cx, m, e) { if (e.kind === "doc" || e.action === "refresh") await draw(cx, m); },
});
async function draw(cx: Cx, m: Mount) {
  const { count } = await cx.query(doc.layers.count({ scope: "active" }));
  await cx.ui.publish(m, ui.col([ui.label(`${count} layers`), ui.button("refresh", "Refresh")]));
}
```

```ts
// 2 EDIT COMMAND   craft.toml: permissions = ["doc.edit"]
export default command({ id: "prefix", label: "Prefix Layers…" }, async (cx) => {
  const { layers } = await cx.query(doc.selection.layers({}));
  const p = await cx.ui.prompt({ title: "Prefix", value: "fx_" });
  if (p) await cx.commit("Prefix layers",
    layers.map(l => doc.layer.rename({ layer: l.id, name: p + l.name })));
}); // A rename by the user meanwhile is in the read set: OpError{code:"Stale", index, op}, no Undo
```

```rust
// 3 NON-DESTRUCTIVE EFFECT (EC LayerSource filter, PC Smart Filter role)
#[craft::props] struct Glow { radius: Px, #[range(0.0, 4.0)] gain: f32 }
#[craft::processor(id = "glow", role = Filter, props = Glow)]
fn plan(r: &ProcessRequest<Glow>) -> ProcessPlan {
    ProcessPlan::tiles().input(0, r.coverage.expand(r.props.radius.ceil_i()))
        .want(ImageDesc::rgba_f32(Color::SceneLinear, Alpha::Associated))
}
```

```rust
// 4 RENDER KERNEL (stage of 3)
#[craft::kernel(processor = "glow")]
fn tile(t: &TileCtx<Glow>, src: &Image, out: &mut ImageMut) {
    let blurred = img::box_blur_separable(src, t.props.radius);
    for (x, y, px) in out.pixels_mut() { *px = src.at(x, y) + blurred.at(x, y).scale(t.props.gain); }
}
```

```rust
// 5 PROCEDURAL LAYER (EC LayerSource::Extension, PC LayerContent::Extension)
#[craft::props] struct Rays { count: u32, color: Rgba }
#[craft::object_type(id = "rays", role = LayerSource, props = Rays, schema = 1)]
fn tile(t: &TileCtx<Rays>, out: &mut ImageMut) {
    let c = t.props.color.premul_in(out.desc().color);
    for (x, y, px) in out.pixels_mut() {
        let a = t.to_doc(x, y).angle_from(t.extent_center());
        *px = c.scale(((a * t.props.count as f32).sin() * 0.5 + 0.5).powi(8));
    }
}
```

```ts
// 6 BRUSH OR TOOL (stroke kept in guest memory, one Undo at end)
let pts: Point[] = [];
export default tool({ id: "scatter", views: ["canvas"], claims: ["drag"] }, {
  begin() { pts = []; },
  async samples(cx, g, s) { pts.push(...s.map(p => p.point));
    await cx.overlay.publish(g, ui.path(pts.slice(-256), { role: "brushPreview" })); },
  end: (cx, g) => cx.commit("Scatter",
    [doc.paint.stamps({ target: g.target, points: pts, brush: { size: px(12) } })]),
  cancel() { pts = []; },
});
```

```ts
// 7 IMPORTER (the host owns the picker, the grant and the edit)
export default format({ id: "ies", extensions: ["ies"], magic: "IESNA" }, {
  async read(cx, h) {
    const lines = new TextDecoder().decode(await cx.fs.lease(h).readAll({ max: mib(4) })).split("\n");
    const tilt = lines.findIndex(l => l.startsWith("TILT="));
    const values = lines.slice(tilt + 3).join(" ").trim().split(/\s+/).map(Number);
    return importPlan.newLayer({ object: { type: "rays", props: { count: values.length } },
                                 assets: [asset.embed(h)] });
  },
});
```
