use craft_plugin::{Key, Modifiers, Paint, Pointer, PointerPhase, Shape, UiEventKind};
use craft_ui::contract::{Props as _, Show as _};
use craft_ui::controls::paint_area::{PaintArea, PaintPhase};
use craft_ui::primitives::ContentPainterCx;
use egui::epaint::{CubicBezierShape, PathShape, PathStroke};
use egui::{Align2, Color32, FontId, Pos2, Rect, Stroke, StrokeKind, Vec2};

use crate::{Painter, values};

const MAX_POINTS: usize = 65_536;
const MAX_TEXT: f32 = 512.0;

fn color(painter: &ContentPainterCx, paint: Paint) -> Color32 {
    let style = painter.style_cx();
    let tokens = &style.tokens.color;
    let token = match paint {
        Paint::Text => tokens.text.normal,
        Paint::Weak => tokens.text.muted,
        Paint::Faint => tokens.text.faint,
        Paint::Accent => tokens.accent.solid,
        Paint::Background => tokens.surface.background,
        Paint::Surface => tokens.surface.panel,
        Paint::Border => tokens.line.border,
        Paint::Error => tokens.status.bad.fg,
        Paint::Good => tokens.status.good.fg,
        Paint::Warn => tokens.status.warn.fg,
        Paint::Selected => tokens.state.selected,
        Paint::Series(n) => tokens.data.series[usize::from(n) % tokens.data.series.len()],
        Paint::Rgba(rgba) => craft_theme::Rgba(rgba),
    };
    craft_ui::style::rgba_color(token)
}

struct Space {
    origin: Pos2,
    scale: Vec2,
}

impl Space {
    fn of(rect: Rect, size: [f32; 2]) -> Space {
        let axis = |room: f32, wanted: f32| if wanted.is_finite() && wanted > 0.0 { room / wanted } else { 1.0 };
        Space { origin: rect.min, scale: Vec2::new(axis(rect.width(), size[0]), axis(rect.height(), size[1])) }
    }

    fn at(&self, [x, y]: [f32; 2]) -> Option<Pos2> {
        (x.is_finite() && y.is_finite()).then(|| self.origin + Vec2::new(x * self.scale.x, y * self.scale.y))
    }

    fn length(&self, length: f32) -> f32 {
        if length.is_finite() { (length * self.scale.x.min(self.scale.y)).clamp(0.0, values::MAX_ROOM) } else { 0.0 }
    }

    fn back(&self, pos: Vec2) -> [f32; 2] {
        [pos.x / self.scale.x, pos.y / self.scale.y]
    }
}

fn shape(painter: &ContentPainterCx, space: &Space, shape: &Shape) -> Option<egui::Shape> {
    let stroke = |paint: Paint, width: f32| Stroke::new(space.length(width), color(painter, paint));
    let points = |points: &[[f32; 2]]| points.iter().take(MAX_POINTS).map(|point| space.at(*point)).collect::<Option<Vec<_>>>();
    Some(match shape {
        Shape::Line { points: line, color: paint, width } => egui::Shape::line(points(line)?, stroke(*paint, *width)),
        Shape::Bezier { points: curve, color: paint, width } => {
            let [a, b, c, d] = [space.at(curve[0])?, space.at(curve[1])?, space.at(curve[2])?, space.at(curve[3])?];
            CubicBezierShape::from_points_stroke([a, b, c, d], false, Color32::TRANSPARENT, stroke(*paint, *width)).into()
        }
        Shape::Rect { min, max, fill, stroke: outline, radius } => {
            let rect = Rect::from_two_pos(space.at(*min)?, space.at(*max)?);
            let fill = fill.map_or(Color32::TRANSPARENT, |paint| color(painter, paint));
            let outline = outline.map_or(Stroke::NONE, |(paint, width)| stroke(paint, width));
            egui::Shape::rect_filled(rect, space.length(*radius), fill).also(egui::Shape::rect_stroke(
                rect,
                space.length(*radius),
                outline,
                StrokeKind::Inside,
            ))
        }
        Shape::Circle { center, radius, fill, stroke: outline } => {
            let fill = fill.map_or(Color32::TRANSPARENT, |paint| color(painter, paint));
            let outline = outline.map_or(Stroke::NONE, |(paint, width)| stroke(paint, width));
            egui::Shape::circle_filled(space.at(*center)?, space.length(*radius), fill).also(egui::Shape::circle_stroke(
                space.at(*center)?,
                space.length(*radius),
                outline,
            ))
        }
        Shape::Text { pos, text, color: paint, size } => {
            let size = if size.is_finite() { space.length(*size).clamp(1.0, MAX_TEXT) } else { 1.0 };
            let galley = painter.painter.layout_no_wrap(text.clone(), FontId::proportional(size), color(painter, *paint));
            let at = Align2::LEFT_TOP.anchor_size(space.at(*pos)?, galley.size()).min;
            egui::Shape::galley(at, galley, color(painter, *paint))
        }
        Shape::Polygon { points: outline, fill } => {
            egui::Shape::Path(PathShape { points: points(outline)?, closed: true, fill: color(painter, *fill), stroke: PathStroke::NONE })
        }
        Shape::Image { .. } => return None,
    })
}

trait Also {
    fn also(self, other: egui::Shape) -> egui::Shape;
}

impl Also for egui::Shape {
    fn also(self, other: egui::Shape) -> egui::Shape {
        egui::Shape::Vec(vec![self, other])
    }
}

fn modifiers(mods: craft_keymap::Mods) -> Modifiers {
    Modifiers { shift: mods.shift, alt: mods.alt, command: mods.cmd }
}

impl Painter<'_> {
    pub(crate) fn canvas(&mut self, ui: &mut egui::Ui, id: u64, size: [f32; 2], shapes: &[Shape], label: craft_ui::runtime::env::Label) {
        let aspect = if size[0] > 0.0 && size[1] > 0.0 { size[0] / size[1] } else { 2.0 };
        let out = PaintArea::new(label, |painter, rect| {
            let space = Space::of(rect, size);
            painter.data_shapes(shapes.iter().filter_map(|drawn| shape(painter, &space, drawn)));
        })
        .aspect(aspect)
        .id_salt(self.salt(id))
        .show(ui);
        let space = Space::of(out.rect, size);
        for sample in &out.pointers {
            let phase = match sample.phase {
                PaintPhase::Down => PointerPhase::Down,
                PaintPhase::Drag => PointerPhase::Drag,
                PaintPhase::Up => PointerPhase::Up,
                _ => PointerPhase::Hover,
            };
            let pointer =
                Pointer { pos: space.back(sample.pos), phase, modifiers: modifiers(sample.mods), scroll: [sample.scroll.x, sample.scroll.y] };
            self.emit(id, UiEventKind::Pointer(pointer));
        }
        for chord in &out.keys {
            self.emit(id, UiEventKind::Key(Key { name: chord.key.to_egui().name().to_string(), modifiers: modifiers(chord.mods) }));
        }
    }
}
