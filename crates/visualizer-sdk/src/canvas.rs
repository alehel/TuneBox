//! The canvas a look draws on: it encodes shapes as the ABI's drawing commands.
//!
//! The canvas leaves out any shape TuneBox would refuse (a position that is not a number or
//! is out of range, more commands or vertices than a frame may have), so a look built with the
//! SDK is never turned off for what it draws; [`Canvas::left_out`] counts what was dropped.

use crate::Color;
use crate::abi::{
    MAX_COMMANDS, MAX_COORDINATE, MAX_DRAWING_BYTES, MAX_INDICES, MAX_STROKE_WIDTH, MAX_VERTICES, OP_FILL_CIRCLE,
    OP_FILL_POLYGON, OP_FILL_RECT, OP_LINE, OP_MESH, OP_STROKE_CIRCLE, OP_STROKE_RECT,
};

/// A mesh vertex: a position and the colour blended from it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vertex {
    pub x: f32,
    pub y: f32,
    pub color: Color,
}

/// One frame's drawing.
#[derive(Debug, Default)]
pub struct Canvas {
    width: f32,
    height: f32,
    words: Vec<u32>,
    commands: usize,
    vertices: usize,
    indices: usize,
    left_out: usize,
}

fn position_ok(value: f32) -> bool {
    value.is_finite() && value.abs() <= MAX_COORDINATE
}

fn size_ok(value: f32) -> bool {
    value.is_finite() && (0.0..=MAX_COORDINATE).contains(&value)
}

fn stroke_ok(value: f32) -> bool {
    value.is_finite() && (0.0..=MAX_STROKE_WIDTH).contains(&value)
}

impl Canvas {
    /// An empty canvas `width` × `height` points.
    pub fn new(width: f32, height: f32) -> Self {
        Self { width, height, ..Self::default() }
    }

    /// The visualizer's width and height in points. The origin is the top-left corner.
    pub fn size(&self) -> (f32, f32) {
        (self.width, self.height)
    }

    /// How many shapes were left out this frame because TuneBox would refuse them.
    pub fn left_out(&self) -> usize {
        self.left_out
    }

    /// The drawing as the ABI's words.
    pub fn words(&self) -> &[u32] {
        &self.words
    }

    /// Adds one command if it is valid and fits, else counts it as left out.
    fn push(&mut self, valid: bool, vertices: usize, indices: usize, words: impl FnOnce(&mut Vec<u32>)) {
        let fits = self.commands < MAX_COMMANDS
            && self.vertices + vertices <= MAX_VERTICES
            && self.indices + indices <= MAX_INDICES;
        if !(valid && fits) {
            self.left_out += 1;
            return;
        }
        let before = self.words.len();
        words(&mut self.words);
        if self.words.len() * 4 > MAX_DRAWING_BYTES {
            self.words.truncate(before);
            self.left_out += 1;
            return;
        }
        self.commands += 1;
        self.vertices += vertices;
        self.indices += indices;
    }

    /// A filled rectangle, `[x, y, width, height]`, with corners rounded by `rounding` points.
    pub fn fill_rect(&mut self, rect: [f32; 4], rounding: f32, color: Color) {
        let [x, y, width, height] = rect;
        let valid = position_ok(x) && position_ok(y) && size_ok(width) && size_ok(height) && size_ok(rounding);
        self.push(valid, 0, 0, |w| {
            w.extend([OP_FILL_RECT, x.to_bits(), y.to_bits(), width.to_bits(), height.to_bits()]);
            w.extend([rounding.to_bits(), color.to_bits()]);
        });
    }

    /// A rectangle's outline, `[x, y, width, height]`, `stroke` points wide.
    pub fn stroke_rect(&mut self, rect: [f32; 4], rounding: f32, stroke: f32, color: Color) {
        let [x, y, width, height] = rect;
        let valid = position_ok(x)
            && position_ok(y)
            && size_ok(width)
            && size_ok(height)
            && size_ok(rounding)
            && stroke_ok(stroke);
        self.push(valid, 0, 0, |w| {
            w.extend([OP_STROKE_RECT, x.to_bits(), y.to_bits(), width.to_bits(), height.to_bits()]);
            w.extend([rounding.to_bits(), stroke.to_bits(), color.to_bits()]);
        });
    }

    /// A filled circle.
    pub fn fill_circle(&mut self, x: f32, y: f32, radius: f32, color: Color) {
        let valid = position_ok(x) && position_ok(y) && size_ok(radius);
        self.push(valid, 0, 0, |w| {
            w.extend([OP_FILL_CIRCLE, x.to_bits(), y.to_bits(), radius.to_bits(), color.to_bits()])
        });
    }

    /// A circle's outline, `stroke` points wide.
    pub fn stroke_circle(&mut self, x: f32, y: f32, radius: f32, stroke: f32, color: Color) {
        let valid = position_ok(x) && position_ok(y) && size_ok(radius) && stroke_ok(stroke);
        self.push(valid, 0, 0, |w| {
            w.extend([OP_STROKE_CIRCLE, x.to_bits(), y.to_bits(), radius.to_bits(), stroke.to_bits(), color.to_bits()]);
        });
    }

    /// A line through `points` (at least two), joined back to the start when `closed`.
    pub fn line(&mut self, points: &[(f32, f32)], closed: bool, stroke: f32, color: Color) {
        let valid =
            points.len() >= 2 && stroke_ok(stroke) && points.iter().all(|&(x, y)| position_ok(x) && position_ok(y));
        self.push(valid, points.len(), 0, |w| {
            w.extend([OP_LINE, stroke.to_bits(), color.to_bits(), u32::from(closed), points.len() as u32]);
            w.extend(points.iter().flat_map(|&(x, y)| [x.to_bits(), y.to_bits()]));
        });
    }

    /// A filled convex polygon through `points` (at least three). A concave outline fills
    /// unpredictably: split it into convex parts or use a [`Canvas::mesh`].
    pub fn fill_polygon(&mut self, points: &[(f32, f32)], color: Color) {
        let valid = points.len() >= 3 && points.iter().all(|&(x, y)| position_ok(x) && position_ok(y));
        self.push(valid, points.len(), 0, |w| {
            w.extend([OP_FILL_POLYGON, color.to_bits(), points.len() as u32]);
            w.extend(points.iter().flat_map(|&(x, y)| [x.to_bits(), y.to_bits()]));
        });
    }

    /// Triangles between `vertices`, three `indices` each, colours blended across each one.
    pub fn mesh(&mut self, vertices: &[Vertex], indices: &[u32]) {
        let valid = indices.len().is_multiple_of(3)
            && indices.iter().all(|&i| (i as usize) < vertices.len())
            && vertices.iter().all(|v| position_ok(v.x) && position_ok(v.y));
        self.push(valid, vertices.len(), indices.len(), |w| {
            w.extend([OP_MESH, vertices.len() as u32, indices.len() as u32]);
            w.extend(vertices.iter().flat_map(|v| [v.x.to_bits(), v.y.to_bits(), v.color.to_bits()]));
            w.extend(indices);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_a_circle() {
        let mut canvas = Canvas::new(100.0, 50.0);
        canvas.fill_circle(1.0, 2.0, 3.0, Color::rgb(255, 0, 0));
        assert_eq!(
            canvas.words(),
            &[OP_FILL_CIRCLE, 1f32.to_bits(), 2f32.to_bits(), 3f32.to_bits(), Color::rgb(255, 0, 0).to_bits()]
        );
    }

    #[test]
    fn leaves_out_what_tunebox_would_refuse() {
        let mut canvas = Canvas::new(100.0, 50.0);
        canvas.fill_circle(f32::NAN, 0.0, 1.0, Color::default());
        canvas.fill_rect([0.0, 0.0, -1.0, 1.0], 0.0, Color::default());
        canvas.line(&[(0.0, 0.0)], false, 1.0, Color::default());
        canvas.mesh(&[Vertex { x: 0.0, y: 0.0, color: Color::default() }], &[0, 0, 1]);
        assert_eq!(canvas.left_out(), 4);
        assert!(canvas.words().is_empty());
        for _ in 0..MAX_COMMANDS + 5 {
            canvas.fill_circle(1.0, 1.0, 1.0, Color::default());
        }
        assert_eq!(canvas.left_out(), 9);
    }
}
