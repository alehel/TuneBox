//! Icons, drawn as vector shapes so they are crisp at any scale and need no image assets.
//!
//! Every function paints into a square `rect` in `color`.

use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, vec2};

/// What an icon shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Glyph {
    Play,
    Pause,
    Previous,
    Next,
    SpeakerQuiet,
    SpeakerLoud,
    Shuffle,
    Repeat,
    RepeatOne,
    Plus,
    Artwork,
    Note,
    /// An open book: audiobooks.
    Book,
    Disc,
    Player,
    Playlist,
    SmartPlaylist,
    Eject,
    Search,
    ListView,
    GridView,
    NowPlaying,
    Visualizer,
    Folder,
    Lock,
    Pencil,
    Info,
    /// A song whose file is gone: the classic exclamation mark.
    Missing,
    /// A song on a drive or share that is not connected: a drive, struck through.
    NotConnected,
}

/// Paints `glyph` into `rect`.
pub fn paint(painter: &Painter, rect: Rect, glyph: Glyph, color: Color32) {
    let r = Square::new(rect);
    match glyph {
        Glyph::Play => triangle(painter, [r.at(0.22, 0.12), r.at(0.88, 0.5), r.at(0.22, 0.88)], color),
        Glyph::Pause => {
            painter.rect_filled(Rect::from_min_max(r.at(0.2, 0.14), r.at(0.42, 0.86)), 1.0, color);
            painter.rect_filled(Rect::from_min_max(r.at(0.58, 0.14), r.at(0.8, 0.86)), 1.0, color);
        }
        Glyph::Previous => {
            triangle(painter, [r.at(0.52, 0.2), r.at(0.08, 0.5), r.at(0.52, 0.8)], color);
            triangle(painter, [r.at(0.94, 0.2), r.at(0.5, 0.5), r.at(0.94, 0.8)], color);
        }
        Glyph::Next => {
            triangle(painter, [r.at(0.06, 0.2), r.at(0.5, 0.5), r.at(0.06, 0.8)], color);
            triangle(painter, [r.at(0.48, 0.2), r.at(0.92, 0.5), r.at(0.48, 0.8)], color);
        }
        Glyph::SpeakerQuiet => speaker(painter, &r, color, 0),
        Glyph::SpeakerLoud => speaker(painter, &r, color, 2),
        Glyph::Shuffle => shuffle(painter, &r, color),
        Glyph::Repeat => repeat(painter, &r, color, false),
        Glyph::RepeatOne => repeat(painter, &r, color, true),
        Glyph::Plus => {
            let stroke = Stroke::new(r.size * 0.14, color);
            painter.line_segment([r.at(0.5, 0.18), r.at(0.5, 0.82)], stroke);
            painter.line_segment([r.at(0.18, 0.5), r.at(0.82, 0.5)], stroke);
        }
        Glyph::Artwork => {
            let stroke = Stroke::new(r.size * 0.09, color);
            painter.rect_stroke(
                Rect::from_min_max(r.at(0.14, 0.2), r.at(0.86, 0.8)),
                1.0,
                stroke,
                egui::StrokeKind::Middle,
            );
            triangle(painter, [r.at(0.22, 0.74), r.at(0.45, 0.42), r.at(0.62, 0.74)], color);
            triangle(painter, [r.at(0.5, 0.74), r.at(0.66, 0.54), r.at(0.8, 0.74)], color);
        }
        Glyph::Note => note(painter, &r, color),
        Glyph::Book => {
            // Two pages, rising to the spine.
            for (outer, inner) in [(0.06, 0.47), (0.94, 0.53)] {
                painter.add(Shape::convex_polygon(
                    vec![r.at(outer, 0.2), r.at(inner, 0.28), r.at(inner, 0.9), r.at(outer, 0.8)],
                    color,
                    Stroke::NONE,
                ));
            }
        }
        Glyph::Disc => {
            // A ring and a hub, so the disc reads in any colour (white on a selection too).
            painter.circle_stroke(r.at(0.5, 0.5), r.size * 0.32, Stroke::new(r.size * 0.2, color));
            painter.circle_filled(r.at(0.5, 0.5), r.size * 0.07, color);
        }
        Glyph::Player => {
            let body = Rect::from_min_max(r.at(0.24, 0.06), r.at(0.76, 0.94));
            painter.rect_filled(body, r.size * 0.1, color);
            painter.rect_filled(Rect::from_min_max(r.at(0.32, 0.14), r.at(0.68, 0.44)), 1.0, Color32::WHITE);
            painter.circle_stroke(r.at(0.5, 0.68), r.size * 0.13, Stroke::new(r.size * 0.07, Color32::WHITE));
        }
        Glyph::Playlist => {
            let stroke = Stroke::new(r.size * 0.1, color);
            for (i, y) in [0.28, 0.5, 0.72].into_iter().enumerate() {
                painter.circle_filled(r.at(0.16, y), r.size * 0.07, color);
                let end = if i == 2 { 0.62 } else { 0.88 };
                painter.line_segment([r.at(0.32, y), r.at(end, y)], stroke);
            }
        }
        Glyph::SmartPlaylist => gear(painter, &r, color),
        Glyph::Eject => {
            triangle(painter, [r.at(0.5, 0.16), r.at(0.86, 0.58), r.at(0.14, 0.58)], color);
            painter.rect_filled(Rect::from_min_max(r.at(0.14, 0.7), r.at(0.86, 0.84)), 1.0, color);
        }
        Glyph::Search => {
            painter.circle_stroke(r.at(0.42, 0.42), r.size * 0.26, Stroke::new(r.size * 0.11, color));
            painter.line_segment([r.at(0.6, 0.6), r.at(0.86, 0.86)], Stroke::new(r.size * 0.14, color));
        }
        Glyph::ListView => {
            let stroke = Stroke::new(r.size * 0.12, color);
            for y in [0.24, 0.5, 0.76] {
                painter.line_segment([r.at(0.12, y), r.at(0.88, y)], stroke);
            }
        }
        Glyph::GridView => {
            for (x, y) in [(0.12, 0.12), (0.56, 0.12), (0.12, 0.56), (0.56, 0.56)] {
                painter.rect_filled(Rect::from_min_size(r.at(x, y), vec2(r.size * 0.32, r.size * 0.32)), 1.0, color);
            }
        }
        Glyph::NowPlaying => speaker(painter, &r, color, 1),
        Glyph::Folder => {
            painter.add(Shape::convex_polygon(
                vec![
                    r.at(0.06, 0.24),
                    r.at(0.38, 0.24),
                    r.at(0.46, 0.33),
                    r.at(0.94, 0.33),
                    r.at(0.94, 0.84),
                    r.at(0.06, 0.84),
                ],
                color,
                Stroke::NONE,
            ));
            painter.rect_filled(
                Rect::from_min_max(r.at(0.06, 0.4), r.at(0.94, 0.42)),
                0.0,
                Color32::from_white_alpha(90),
            );
        }
        Glyph::Pencil => {
            painter.add(Shape::convex_polygon(
                vec![r.at(0.62, 0.14), r.at(0.86, 0.38), r.at(0.38, 0.86), r.at(0.14, 0.86), r.at(0.14, 0.62)],
                color,
                Stroke::NONE,
            ));
            painter.line_segment(
                [r.at(0.55, 0.25), r.at(0.75, 0.45)],
                Stroke::new(r.size * 0.07, Color32::from_white_alpha(170)),
            );
        }
        Glyph::Info => {
            painter.circle_filled(r.at(0.5, 0.5), r.size * 0.46, color);
            painter.circle_filled(r.at(0.5, 0.3), r.size * 0.07, Color32::WHITE);
            painter.rect_filled(Rect::from_min_max(r.at(0.43, 0.42), r.at(0.57, 0.76)), r.size * 0.04, Color32::WHITE);
        }
        Glyph::Missing => {
            painter.circle_filled(r.at(0.5, 0.5), r.size * 0.46, color);
            painter.rect_filled(Rect::from_min_max(r.at(0.42, 0.2), r.at(0.58, 0.6)), r.size * 0.05, Color32::WHITE);
            painter.circle_filled(r.at(0.5, 0.74), r.size * 0.08, Color32::WHITE);
        }
        Glyph::NotConnected => {
            let stroke = Stroke::new(r.size * 0.1, color);
            painter.rect_stroke(
                Rect::from_min_max(r.at(0.12, 0.3), r.at(0.88, 0.72)),
                r.size * 0.1,
                stroke,
                egui::StrokeKind::Middle,
            );
            painter.circle_filled(r.at(0.7, 0.51), r.size * 0.06, color);
            painter.line_segment([r.at(0.12, 0.1), r.at(0.88, 0.92)], stroke);
        }
        Glyph::Lock => {
            painter.circle_stroke(r.at(0.5, 0.38), r.size * 0.2, Stroke::new(r.size * 0.12, color));
            painter.rect_filled(Rect::from_min_max(r.at(0.18, 0.45), r.at(0.82, 0.92)), r.size * 0.1, color);
        }
        Glyph::Visualizer => {
            for (i, height) in [0.35, 0.7, 0.5, 0.85, 0.45].into_iter().enumerate() {
                let x = 0.1 + i as f32 * 0.17;
                painter.rect_filled(Rect::from_min_max(r.at(x, 0.92 - height), r.at(x + 0.12, 0.92)), 1.0, color);
            }
        }
    }
}

/// Maps unit coordinates onto a square.
struct Square {
    min: Pos2,
    size: f32,
}

impl Square {
    fn new(rect: Rect) -> Self {
        let size = rect.width().min(rect.height());
        Self { min: rect.center() - vec2(size, size) / 2.0, size }
    }

    fn at(&self, x: f32, y: f32) -> Pos2 {
        self.min + vec2(x, y) * self.size
    }
}

fn triangle(painter: &Painter, points: [Pos2; 3], color: Color32) {
    painter.add(Shape::convex_polygon(points.to_vec(), color, Stroke::NONE));
}

fn speaker(painter: &Painter, r: &Square, color: Color32, waves: usize) {
    painter.add(Shape::convex_polygon(
        vec![
            r.at(0.08, 0.36),
            r.at(0.26, 0.36),
            r.at(0.48, 0.16),
            r.at(0.48, 0.84),
            r.at(0.26, 0.64),
            r.at(0.08, 0.64),
        ],
        color,
        Stroke::NONE,
    ));
    for wave in 0..waves {
        let radius = r.size * (0.18 + 0.16 * wave as f32);
        arc(painter, r.at(0.48, 0.5), radius, -0.9, 0.9, Stroke::new(r.size * 0.08, color));
    }
}

fn arc(painter: &Painter, center: Pos2, radius: f32, from: f32, to: f32, stroke: Stroke) {
    let points: Vec<Pos2> = (0..=12)
        .map(|i| {
            let a = from + (to - from) * i as f32 / 12.0;
            center + vec2(a.cos(), a.sin()) * radius
        })
        .collect();
    painter.add(Shape::line(points, stroke));
}

fn shuffle(painter: &Painter, r: &Square, color: Color32) {
    let stroke = Stroke::new(r.size * 0.09, color);
    painter.add(Shape::line(vec![r.at(0.08, 0.28), r.at(0.3, 0.28), r.at(0.62, 0.72), r.at(0.8, 0.72)], stroke));
    painter.add(Shape::line(vec![r.at(0.08, 0.72), r.at(0.3, 0.72), r.at(0.62, 0.28), r.at(0.8, 0.28)], stroke));
    triangle(painter, [r.at(0.76, 0.14), r.at(0.96, 0.28), r.at(0.76, 0.42)], color);
    triangle(painter, [r.at(0.76, 0.58), r.at(0.96, 0.72), r.at(0.76, 0.86)], color);
}

fn repeat(painter: &Painter, r: &Square, color: Color32, one: bool) {
    let stroke = Stroke::new(r.size * 0.09, color);
    painter.add(Shape::line(vec![r.at(0.2, 0.62), r.at(0.2, 0.3), r.at(0.78, 0.3)], stroke));
    painter.add(Shape::line(vec![r.at(0.8, 0.38), r.at(0.8, 0.7), r.at(0.22, 0.7)], stroke));
    triangle(painter, [r.at(0.7, 0.16), r.at(0.92, 0.3), r.at(0.7, 0.44)], color);
    triangle(painter, [r.at(0.3, 0.56), r.at(0.08, 0.7), r.at(0.3, 0.84)], color);
    if one {
        painter.circle_filled(r.at(0.5, 0.5), r.size * 0.17, color);
        painter.text(
            r.at(0.5, 0.5),
            egui::Align2::CENTER_CENTER,
            "1",
            egui::FontId::proportional(r.size * 0.28),
            Color32::WHITE,
        );
    }
}

fn note(painter: &Painter, r: &Square, color: Color32) {
    let stroke = Stroke::new(r.size * 0.09, color);
    painter.circle_filled(r.at(0.3, 0.76), r.size * 0.14, color);
    painter.circle_filled(r.at(0.76, 0.66), r.size * 0.14, color);
    painter.line_segment([r.at(0.42, 0.76), r.at(0.42, 0.2)], stroke);
    painter.line_segment([r.at(0.88, 0.66), r.at(0.88, 0.1)], stroke);
    painter.add(Shape::convex_polygon(
        vec![r.at(0.38, 0.16), r.at(0.92, 0.06), r.at(0.92, 0.2), r.at(0.38, 0.3)],
        color,
        Stroke::NONE,
    ));
}

fn gear(painter: &Painter, r: &Square, color: Color32) {
    let center = r.at(0.5, 0.5);
    for i in 0..8 {
        let a = i as f32 * std::f32::consts::TAU / 8.0;
        let dir = vec2(a.cos(), a.sin());
        painter.line_segment(
            [center + dir * r.size * 0.2, center + dir * r.size * 0.44],
            Stroke::new(r.size * 0.16, color),
        );
    }
    painter.circle_filled(center, r.size * 0.3, color);
    painter.circle_filled(center, r.size * 0.12, Color32::WHITE);
}

/// A `size`-sided square centred in `rect`.
pub fn centered(rect: Rect, size: f32) -> Rect {
    Rect::from_center_size(rect.center(), vec2(size, size))
}
