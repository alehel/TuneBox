//! Drawing helpers the looks share: colours and gradient-filled shapes.

use egui::ecolor::Hsva;
use egui::epaint::{Mesh, Vertex, WHITE_UV};
use egui::{Color32, Painter, Pos2, Rect};

/// A colour from hue (0–1, wrapping), saturation, value and alpha.
pub fn hue(h: f32, s: f32, v: f32, alpha: f32) -> Color32 {
    Color32::from(Hsva::new(h.rem_euclid(1.0), s, v, alpha))
}

/// Adds a quad with a colour per corner (top-left, top-right, bottom-right, bottom-left), which
/// the GPU blends across the shape: gradients and glows.
pub fn quad(mesh: &mut Mesh, corners: [Pos2; 4], colors: [Color32; 4]) {
    let base = mesh.vertices.len() as u32;
    for (pos, color) in corners.into_iter().zip(colors) {
        mesh.vertices.push(Vertex { pos, uv: WHITE_UV, color });
    }
    mesh.add_triangle(base, base + 1, base + 2);
    mesh.add_triangle(base, base + 2, base + 3);
}

/// Fills `rect` with a vertical gradient.
pub fn background(painter: &Painter, rect: Rect, top: Color32, bottom: Color32) {
    let mut mesh = Mesh::default();
    quad(
        &mut mesh,
        [rect.left_top(), rect.right_top(), rect.right_bottom(), rect.left_bottom()],
        [top, top, bottom, bottom],
    );
    painter.add(mesh);
}
