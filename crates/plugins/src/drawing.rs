//! A plug-in's drawing: the ABI's command words decoded into shapes, and every value checked.
//!
//! This is the boundary where a stranger's numbers become TuneBox's shapes, so nothing passes
//! unchecked: counts are bounded before anything is allocated, positions must be finite and in
//! range, indices must name a vertex, and the whole drawing must fit the per-frame limits in
//! [`abi`](tunebox_visualizer_sdk::abi). A drawing that breaks any rule is refused as a whole.

use tunebox_visualizer_sdk::abi::{
    MAX_COMMANDS, MAX_COORDINATE, MAX_DRAWING_BYTES, MAX_INDICES, MAX_STROKE_WIDTH, MAX_VERTICES, OP_FILL_CIRCLE,
    OP_FILL_POLYGON, OP_FILL_RECT, OP_LINE, OP_MESH, OP_STROKE_CIRCLE, OP_STROKE_RECT,
};

/// A position in points from the visualizer's top-left corner.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

/// A rectangle: its top-left corner and size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// An sRGB colour with alpha, not premultiplied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

/// A mesh vertex.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeshVertex {
    pub pos: Point,
    pub color: Rgba,
}

/// One shape.
#[derive(Clone, Debug, PartialEq)]
pub enum DrawCommand {
    FillRect {
        rect: Rect,
        rounding: f32,
        color: Rgba,
    },
    StrokeRect {
        rect: Rect,
        rounding: f32,
        width: f32,
        color: Rgba,
    },
    FillCircle {
        center: Point,
        radius: f32,
        color: Rgba,
    },
    StrokeCircle {
        center: Point,
        radius: f32,
        width: f32,
        color: Rgba,
    },
    Line {
        points: Vec<Point>,
        closed: bool,
        width: f32,
        color: Rgba,
    },
    /// Convex.
    FillPolygon {
        points: Vec<Point>,
        color: Rgba,
    },
    /// Triangles: `indices` in threes, each less than `vertices.len()`.
    Mesh {
        vertices: Vec<MeshVertex>,
        indices: Vec<u32>,
    },
}

/// Why a drawing was refused. `command` counts from 1.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum InvalidDrawing {
    #[error("the drawing is {0} bytes, more than the {MAX_DRAWING_BYTES} allowed")]
    TooLarge(usize),
    #[error("the drawing's length, {0} bytes, is not a whole number of 4-byte words")]
    NotWords(usize),
    #[error("command {command} is of an unknown kind ({opcode})")]
    UnknownCommand { command: usize, opcode: u32 },
    #[error("command {command} is cut off")]
    Truncated { command: usize },
    #[error("command {command} has a value that is not a number")]
    NotANumber { command: usize },
    #[error("command {command} has a value out of range")]
    OutOfRange { command: usize },
    #[error("command {command} has too few points")]
    TooFewPoints { command: usize },
    #[error("command {command} has a mesh index that names no vertex, or indices not in threes")]
    BadIndex { command: usize },
    #[error("the drawing lies outside the plug-in's memory")]
    OutsideMemory,
    #[error("more than {MAX_COMMANDS} commands")]
    TooManyCommands,
    #[error("more than {MAX_VERTICES} points and vertices")]
    TooManyVertices,
    #[error("more than {MAX_INDICES} mesh indices")]
    TooManyIndices,
}

/// Decodes and checks a drawing (the bytes after its length word).
pub fn decode(bytes: &[u8]) -> Result<Vec<DrawCommand>, InvalidDrawing> {
    if bytes.len() > MAX_DRAWING_BYTES {
        return Err(InvalidDrawing::TooLarge(bytes.len()));
    }
    if !bytes.len().is_multiple_of(4) {
        return Err(InvalidDrawing::NotWords(bytes.len()));
    }
    let words: Vec<u32> = bytes.chunks_exact(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
    let mut reader = Reader { words: &words, at: 0, command: 0, vertices: 0, indices: 0 };
    let mut commands = Vec::new();
    while reader.at < words.len() {
        if commands.len() >= MAX_COMMANDS {
            return Err(InvalidDrawing::TooManyCommands);
        }
        reader.command += 1;
        commands.push(reader.command()?);
    }
    Ok(commands)
}

/// Walks the words, keeping the running totals the limits apply to.
struct Reader<'a> {
    words: &'a [u32],
    at: usize,
    /// The command being read, from 1.
    command: usize,
    vertices: usize,
    indices: usize,
}

impl Reader<'_> {
    fn word(&mut self) -> Result<u32, InvalidDrawing> {
        let word = self.words.get(self.at).copied().ok_or(InvalidDrawing::Truncated { command: self.command })?;
        self.at += 1;
        Ok(word)
    }

    fn number(&mut self) -> Result<f32, InvalidDrawing> {
        let value = f32::from_bits(self.word()?);
        if value.is_finite() { Ok(value) } else { Err(InvalidDrawing::NotANumber { command: self.command }) }
    }

    fn in_range(&mut self, range: std::ops::RangeInclusive<f32>) -> Result<f32, InvalidDrawing> {
        let value = self.number()?;
        if range.contains(&value) { Ok(value) } else { Err(InvalidDrawing::OutOfRange { command: self.command }) }
    }

    fn coordinate(&mut self) -> Result<f32, InvalidDrawing> {
        self.in_range(-MAX_COORDINATE..=MAX_COORDINATE)
    }

    /// A size, radius or rounding.
    fn size(&mut self) -> Result<f32, InvalidDrawing> {
        self.in_range(0.0..=MAX_COORDINATE)
    }

    fn stroke(&mut self) -> Result<f32, InvalidDrawing> {
        self.in_range(0.0..=MAX_STROKE_WIDTH)
    }

    fn point(&mut self) -> Result<Point, InvalidDrawing> {
        Ok(Point { x: self.coordinate()?, y: self.coordinate()? })
    }

    fn rect(&mut self) -> Result<Rect, InvalidDrawing> {
        Ok(Rect { x: self.coordinate()?, y: self.coordinate()?, width: self.size()?, height: self.size()? })
    }

    fn color(&mut self) -> Result<Rgba, InvalidDrawing> {
        let [r, g, b, a] = self.word()?.to_le_bytes();
        Ok(Rgba { r, g, b, a })
    }

    /// A count of items `words_each` words long, checked against what is left before anything
    /// is allocated for them.
    fn count(&mut self, words_each: usize) -> Result<usize, InvalidDrawing> {
        let count = self.word()? as usize;
        let left = self.words.len().saturating_sub(self.at);
        match count.checked_mul(words_each) {
            Some(needed) if needed <= left => Ok(count),
            _ => Err(InvalidDrawing::Truncated { command: self.command }),
        }
    }

    fn add_vertices(&mut self, count: usize) -> Result<(), InvalidDrawing> {
        self.vertices = self.vertices.saturating_add(count);
        if self.vertices > MAX_VERTICES { Err(InvalidDrawing::TooManyVertices) } else { Ok(()) }
    }

    fn points(&mut self, minimum: usize) -> Result<Vec<Point>, InvalidDrawing> {
        let count = self.count(2)?;
        if count < minimum {
            return Err(InvalidDrawing::TooFewPoints { command: self.command });
        }
        self.add_vertices(count)?;
        (0..count).map(|_| self.point()).collect()
    }

    fn command(&mut self) -> Result<DrawCommand, InvalidDrawing> {
        let command = self.command;
        Ok(match self.word()? {
            OP_FILL_RECT => DrawCommand::FillRect { rect: self.rect()?, rounding: self.size()?, color: self.color()? },
            OP_STROKE_RECT => DrawCommand::StrokeRect {
                rect: self.rect()?,
                rounding: self.size()?,
                width: self.stroke()?,
                color: self.color()?,
            },
            OP_FILL_CIRCLE => {
                DrawCommand::FillCircle { center: self.point()?, radius: self.size()?, color: self.color()? }
            }
            OP_STROKE_CIRCLE => DrawCommand::StrokeCircle {
                center: self.point()?,
                radius: self.size()?,
                width: self.stroke()?,
                color: self.color()?,
            },
            OP_LINE => {
                let width = self.stroke()?;
                let color = self.color()?;
                let closed = match self.word()? {
                    0 => false,
                    1 => true,
                    _ => return Err(InvalidDrawing::OutOfRange { command }),
                };
                DrawCommand::Line { points: self.points(2)?, closed, width, color }
            }
            OP_FILL_POLYGON => {
                let color = self.color()?;
                DrawCommand::FillPolygon { points: self.points(3)?, color }
            }
            OP_MESH => self.mesh()?,
            opcode => return Err(InvalidDrawing::UnknownCommand { command, opcode }),
        })
    }

    fn mesh(&mut self) -> Result<DrawCommand, InvalidDrawing> {
        let command = self.command;
        let vertex_count = self.word()? as usize;
        let index_count = self.word()? as usize;
        let left = self.words.len().saturating_sub(self.at);
        let needed = vertex_count.checked_mul(3).and_then(|v| v.checked_add(index_count));
        if needed.is_none_or(|needed| needed > left) {
            return Err(InvalidDrawing::Truncated { command });
        }
        if !index_count.is_multiple_of(3) {
            return Err(InvalidDrawing::BadIndex { command });
        }
        self.add_vertices(vertex_count)?;
        self.indices = self.indices.saturating_add(index_count);
        if self.indices > MAX_INDICES {
            return Err(InvalidDrawing::TooManyIndices);
        }
        let vertices = (0..vertex_count)
            .map(|_| Ok(MeshVertex { pos: self.point()?, color: self.color()? }))
            .collect::<Result<Vec<_>, InvalidDrawing>>()?;
        let indices = (0..index_count).map(|_| self.word()).collect::<Result<Vec<_>, _>>()?;
        if indices.iter().any(|&i| i as usize >= vertex_count) {
            return Err(InvalidDrawing::BadIndex { command });
        }
        Ok(DrawCommand::Mesh { vertices, indices })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes(words: &[u32]) -> Vec<u8> {
        words.iter().flat_map(|w| w.to_le_bytes()).collect()
    }

    fn f(value: f32) -> u32 {
        value.to_bits()
    }

    const RED: u32 = 0xff00_00ff;

    #[test]
    fn decodes_every_kind_of_command() {
        let words = [
            &[OP_FILL_RECT, f(1.0), f(2.0), f(3.0), f(4.0), f(0.5), RED][..],
            &[OP_STROKE_RECT, f(1.0), f(2.0), f(3.0), f(4.0), f(0.0), f(2.0), RED],
            &[OP_FILL_CIRCLE, f(5.0), f(6.0), f(7.0), RED],
            &[OP_STROKE_CIRCLE, f(5.0), f(6.0), f(7.0), f(1.0), RED],
            &[OP_LINE, f(1.5), RED, 1, 2, f(0.0), f(0.0), f(10.0), f(10.0)],
            &[OP_FILL_POLYGON, RED, 3, f(0.0), f(0.0), f(10.0), f(0.0), f(5.0), f(5.0)],
            &[OP_MESH, 3, 3, f(0.0), f(0.0), RED, f(1.0), f(0.0), RED, f(0.0), f(1.0), RED, 0, 1, 2],
        ]
        .concat();
        let commands = decode(&bytes(&words)).unwrap();
        assert_eq!(commands.len(), 7);
        let red = Rgba { r: 255, g: 0, b: 0, a: 255 };
        assert_eq!(
            commands[0],
            DrawCommand::FillRect { rect: Rect { x: 1.0, y: 2.0, width: 3.0, height: 4.0 }, rounding: 0.5, color: red }
        );
        assert!(matches!(&commands[4], DrawCommand::Line { points, closed: true, .. } if points.len() == 2));
        assert!(
            matches!(&commands[6], DrawCommand::Mesh { vertices, indices } if vertices.len() == 3 && indices.len() == 3)
        );
        assert_eq!(decode(&[]).unwrap(), vec![]);
    }

    #[test]
    fn refuses_values_that_are_not_numbers_or_out_of_range() {
        let nan = [OP_FILL_CIRCLE, f(f32::NAN), f(0.0), f(1.0), RED];
        assert_eq!(decode(&bytes(&nan)), Err(InvalidDrawing::NotANumber { command: 1 }));
        let infinite = [OP_FILL_CIRCLE, f(0.0), f(f32::INFINITY), f(1.0), RED];
        assert_eq!(decode(&bytes(&infinite)), Err(InvalidDrawing::NotANumber { command: 1 }));
        let far = [OP_FILL_CIRCLE, f(MAX_COORDINATE * 2.0), f(0.0), f(1.0), RED];
        assert_eq!(decode(&bytes(&far)), Err(InvalidDrawing::OutOfRange { command: 1 }));
        let negative = [OP_FILL_RECT, f(0.0), f(0.0), f(-1.0), f(1.0), f(0.0), RED];
        assert_eq!(decode(&bytes(&negative)), Err(InvalidDrawing::OutOfRange { command: 1 }));
        let wide = [OP_STROKE_CIRCLE, f(0.0), f(0.0), f(1.0), f(MAX_STROKE_WIDTH + 1.0), RED];
        assert_eq!(decode(&bytes(&wide)), Err(InvalidDrawing::OutOfRange { command: 1 }));
        let flag = [OP_LINE, f(1.0), RED, 7, 2, f(0.0), f(0.0), f(1.0), f(1.0)];
        assert_eq!(decode(&bytes(&flag)), Err(InvalidDrawing::OutOfRange { command: 1 }));
    }

    #[test]
    fn refuses_malformed_structure() {
        assert_eq!(decode(&[1, 2, 3]), Err(InvalidDrawing::NotWords(3)));
        assert_eq!(decode(&bytes(&[99])), Err(InvalidDrawing::UnknownCommand { command: 1, opcode: 99 }));
        let cut = [OP_FILL_CIRCLE, f(0.0), f(0.0)];
        assert_eq!(decode(&bytes(&cut)), Err(InvalidDrawing::Truncated { command: 1 }));
        // A huge count must be refused before anything is allocated for it.
        let huge = [OP_LINE, f(1.0), RED, 0, u32::MAX];
        assert_eq!(decode(&bytes(&huge)), Err(InvalidDrawing::Truncated { command: 1 }));
        let huge_mesh = [OP_MESH, u32::MAX, u32::MAX];
        assert_eq!(decode(&bytes(&huge_mesh)), Err(InvalidDrawing::Truncated { command: 1 }));
        let one_point = [OP_LINE, f(1.0), RED, 0, 1, f(0.0), f(0.0)];
        assert_eq!(decode(&bytes(&one_point)), Err(InvalidDrawing::TooFewPoints { command: 1 }));
        let bad_index = [OP_MESH, 1, 3, f(0.0), f(0.0), RED, 0, 0, 1];
        assert_eq!(decode(&bytes(&bad_index)), Err(InvalidDrawing::BadIndex { command: 1 }));
        let not_threes = [OP_MESH, 1, 2, f(0.0), f(0.0), RED, 0, 0];
        assert_eq!(decode(&bytes(&not_threes)), Err(InvalidDrawing::BadIndex { command: 1 }));
        assert_eq!(decode(&vec![0; MAX_DRAWING_BYTES + 4]), Err(InvalidDrawing::TooLarge(MAX_DRAWING_BYTES + 4)));
    }

    #[test]
    fn enforces_the_per_frame_limits() {
        let circle = [OP_FILL_CIRCLE, f(0.0), f(0.0), f(1.0), RED];
        let too_many: Vec<u32> = circle.iter().copied().cycle().take(circle.len() * (MAX_COMMANDS + 1)).collect();
        assert_eq!(decode(&bytes(&too_many)), Err(InvalidDrawing::TooManyCommands));
        let at_limit: Vec<u32> = circle.iter().copied().cycle().take(circle.len() * MAX_COMMANDS).collect();
        assert_eq!(decode(&bytes(&at_limit)).map(|c| c.len()), Ok(MAX_COMMANDS));

        // Two lines of just over half the vertex limit each.
        let half = MAX_VERTICES / 2 + 1;
        let mut words = Vec::new();
        for _ in 0..2 {
            words.extend([OP_LINE, f(1.0), RED, 0, half as u32]);
            words.extend(std::iter::repeat_n(f(0.0), half * 2));
        }
        assert_eq!(decode(&bytes(&words)), Err(InvalidDrawing::TooManyVertices));
    }
}
