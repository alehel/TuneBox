//! A plug-in's drawing as egui shapes, moved from the plug-in's coordinates (origin at the
//! visualizer's top-left corner) to the screen's. The drawing was checked when it was decoded
//! (`tunebox_engine::plugins::drawing`), so every value here is finite and in range.

use egui::epaint::{CornerRadius, Mesh, Vertex, WHITE_UV};
use egui::{Color32, Pos2, Shape, Stroke, StrokeKind, Vec2, pos2, vec2};
use tunebox_engine::plugins::{DrawCommand, Point, Rect, Rgba};

/// The shapes for `commands`, with the plug-in's origin at `origin`.
pub fn to_shapes(commands: Vec<DrawCommand>, origin: Pos2) -> Vec<Shape> {
    let offset = origin.to_vec2();
    commands.into_iter().map(|command| shape(command, offset)).collect()
}

fn color(rgba: Rgba) -> Color32 {
    Color32::from_rgba_unmultiplied(rgba.r, rgba.g, rgba.b, rgba.a)
}

fn point(p: Point, offset: Vec2) -> Pos2 {
    pos2(p.x, p.y) + offset
}

fn rect(r: Rect, offset: Vec2) -> egui::Rect {
    egui::Rect::from_min_size(pos2(r.x, r.y) + offset, vec2(r.width, r.height))
}

/// egui rounds corners in whole points up to 255.
fn corners(rounding: f32) -> CornerRadius {
    CornerRadius::same(rounding.clamp(0.0, 255.0) as u8)
}

fn shape(command: DrawCommand, offset: Vec2) -> Shape {
    match command {
        DrawCommand::FillRect { rect: r, rounding, color: c } => {
            Shape::rect_filled(rect(r, offset), corners(rounding), color(c))
        }
        DrawCommand::StrokeRect { rect: r, rounding, width, color: c } => {
            Shape::rect_stroke(rect(r, offset), corners(rounding), Stroke::new(width, color(c)), StrokeKind::Middle)
        }
        DrawCommand::FillCircle { center, radius, color: c } => {
            Shape::circle_filled(point(center, offset), radius, color(c))
        }
        DrawCommand::StrokeCircle { center, radius, width, color: c } => {
            Shape::circle_stroke(point(center, offset), radius, Stroke::new(width, color(c)))
        }
        DrawCommand::Line { points, closed, width, color: c } => {
            let points: Vec<Pos2> = points.into_iter().map(|p| point(p, offset)).collect();
            let stroke = Stroke::new(width, color(c));
            if closed { Shape::closed_line(points, stroke) } else { Shape::line(points, stroke) }
        }
        DrawCommand::FillPolygon { points, color: c } => {
            Shape::convex_polygon(points.into_iter().map(|p| point(p, offset)).collect(), color(c), Stroke::NONE)
        }
        DrawCommand::Mesh { vertices, indices } => {
            let vertices = vertices
                .into_iter()
                .map(|v| Vertex { pos: point(v.pos, offset), uv: WHITE_UV, color: color(v.color) })
                .collect();
            Shape::mesh(Mesh { indices, vertices, ..Mesh::default() })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tunebox_engine::plugins::MeshVertex;

    const RED: Rgba = Rgba { r: 255, g: 0, b: 0, a: 255 };

    #[test]
    fn shapes_move_to_the_visualizer_and_keep_their_colours() {
        let commands = vec![
            DrawCommand::FillCircle { center: Point { x: 10.0, y: 20.0 }, radius: 5.0, color: RED },
            DrawCommand::FillRect {
                rect: Rect { x: 0.0, y: 0.0, width: 4.0, height: 2.0 },
                rounding: 900.0,
                color: RED,
            },
            DrawCommand::Line {
                points: vec![Point { x: 0.0, y: 0.0 }, Point { x: 1.0, y: 1.0 }],
                closed: false,
                width: 2.0,
                color: Rgba { r: 0, g: 0, b: 255, a: 255 },
            },
            DrawCommand::Mesh {
                vertices: vec![MeshVertex { pos: Point { x: 1.0, y: 2.0 }, color: RED }; 3],
                indices: vec![0, 1, 2],
            },
        ];
        let shapes = to_shapes(commands, pos2(100.0, 200.0));
        let Shape::Circle(circle) = &shapes[0] else { panic!("{:?}", shapes[0]) };
        assert_eq!((circle.center, circle.radius, circle.fill), (pos2(110.0, 220.0), 5.0, Color32::RED));
        let Shape::Rect(r) = &shapes[1] else { panic!("{:?}", shapes[1]) };
        assert_eq!((r.rect.min, r.corner_radius), (pos2(100.0, 200.0), CornerRadius::same(255)));
        let Shape::Path(path) = &shapes[2] else { panic!("{:?}", shapes[2]) };
        assert_eq!((path.points[1], path.closed, path.stroke.width), (pos2(101.0, 201.0), false, 2.0));
        let Shape::Mesh(mesh) = &shapes[3] else { panic!("{:?}", shapes[3]) };
        assert_eq!(mesh.vertices[0].pos, pos2(101.0, 202.0));
    }
}
