//! SVG path data (`M20 6 9 17l-5-5`), parsed strictly, with the byte where
//! the data stops making sense.
//!
//! The grammar is SVG 2's (section 9.3.9, "The grammar for path data"):
//! every command, absolute and relative (`M L H V C S Q T A Z`), implicit
//! repeats (`M` repeated is `L`), numbers with an optional sign, fraction and
//! exponent written with no separator where the grammar allows (`M.5.5`,
//! `10-5`), flags as one character (`a1 1 0 0110 10`), and separators of
//! white space with at most one comma. Data must start with a move. An empty
//! string (or white space only) is an empty path, which draws nothing.
//!
//! Why not `zeno`'s own parser (the paper's 2.2 assumed it): its validator
//! accepts data it then drops silently (`M 10`, `M10 10 L`, `M10 10 L5 5 6`
//! all validate and lose their last command), accepts data that does not
//! start with a move, and refuses exponents. Strict parsing with the byte is
//! what `Path::from_svg` promises; the commands it makes are `zeno`'s, and an
//! arc becomes cubic curves by `zeno`'s own arc (see [`super::Commands`]).

use super::Commands;
use crate::geometry::Point;

/// Where and why path data stops making sense.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SyntaxError {
    /// The byte offset in the data: the byte that is wrong, or the data's
    /// length when it ends too soon.
    pub(crate) byte: usize,
    pub(crate) reason: &'static str,
}

struct Parser<'a> {
    bytes: &'a [u8],
    position: usize,
}

fn is_white_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | b'\x0c')
}

/// The previous command, for the reflected control point of `S` and `T`.
#[derive(Clone, Copy, PartialEq)]
enum Previous {
    Cubic(Point),
    Quadratic(Point),
    Other,
}

impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.position).copied()
    }

    fn error(&self, reason: &'static str) -> SyntaxError {
        SyntaxError {
            byte: self.position,
            reason,
        }
    }

    fn skip_white_space(&mut self) {
        while self.peek().is_some_and(is_white_space) {
            self.position += 1;
        }
    }

    /// `wsp* ","? wsp*` between two arguments. Answers whether there was a
    /// comma: an argument must follow one.
    fn skip_separator(&mut self) -> bool {
        self.skip_white_space();
        let comma = self.peek() == Some(b',');
        if comma {
            self.position += 1;
            self.skip_white_space();
        }
        comma
    }

    /// Whether a number starts here (the start of another argument group).
    fn at_number(&self) -> bool {
        self.peek()
            .is_some_and(|byte| byte.is_ascii_digit() || matches!(byte, b'+' | b'-' | b'.'))
    }

    fn digits(&mut self) -> usize {
        let start = self.position;
        while self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
            self.position += 1;
        }
        self.position - start
    }

    /// `sign? (digits ("." digits?)? | "." digits) exponent?`.
    fn number(&mut self) -> Result<f32, SyntaxError> {
        let start = self.position;
        if matches!(self.peek(), Some(b'+' | b'-')) {
            self.position += 1;
        }
        let whole = self.digits();
        let mut fraction = 0;
        if self.peek() == Some(b'.') {
            let dot = self.position;
            self.position += 1;
            fraction = self.digits();
            if whole == 0 && fraction == 0 {
                self.position = dot;
            }
        }
        if whole == 0 && fraction == 0 {
            self.position = start;
            return Err(self.error("expected a number"));
        }
        // An exponent only when digits follow it: `1e` is the number 1 and
        // then an `e`, which is not a command.
        if matches!(self.peek(), Some(b'e' | b'E')) {
            let before = self.position;
            self.position += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.position += 1;
            }
            if self.digits() == 0 {
                self.position = before;
            }
        }
        let text =
            std::str::from_utf8(&self.bytes[start..self.position]).expect("a number is ASCII");
        match text.parse::<f32>() {
            Ok(value) if value.is_finite() => Ok(value),
            _ => {
                self.position = start;
                Err(self.error("a number too large to draw"))
            }
        }
    }

    fn flag(&mut self) -> Result<bool, SyntaxError> {
        match self.peek() {
            Some(b'0') => {
                self.position += 1;
                Ok(false)
            }
            Some(b'1') => {
                self.position += 1;
                Ok(true)
            }
            _ => Err(self.error("expected a flag (0 or 1)")),
        }
    }

    /// `count` numbers with separators between them.
    fn numbers<const COUNT: usize>(&mut self) -> Result<[f32; COUNT], SyntaxError> {
        let mut values = [0.0; COUNT];
        for (index, value) in values.iter_mut().enumerate() {
            if index > 0 {
                self.skip_separator();
            }
            *value = self.number()?;
        }
        Ok(values)
    }

    /// rx ry rotation large-arc sweep x y.
    fn arc_arguments(&mut self) -> Result<([f32; 3], bool, bool, [f32; 2]), SyntaxError> {
        let radii_and_rotation = self.numbers::<3>()?;
        self.skip_separator();
        let large = self.flag()?;
        self.skip_separator();
        let sweep = self.flag()?;
        self.skip_separator();
        let end = self.numbers::<2>()?;
        Ok((radii_and_rotation, large, sweep, end))
    }
}

/// Parses `data` into `commands`.
pub(crate) fn parse(data: &str, commands: &mut Commands) -> Result<(), SyntaxError> {
    let mut parser = Parser {
        bytes: data.as_bytes(),
        position: 0,
    };
    let mut previous = Previous::Other;
    let mut first = true;
    loop {
        parser.skip_white_space();
        let Some(letter) = parser.peek() else {
            return Ok(());
        };
        if !matches!(
            letter.to_ascii_uppercase(),
            b'M' | b'L' | b'H' | b'V' | b'C' | b'S' | b'Q' | b'T' | b'A' | b'Z'
        ) {
            return Err(parser.error(
                "expected a command letter (M, L, H, V, C, S, Q, T, A or Z, or m, l, h, v, c, s, \
                 q, t, a or z)",
            ));
        }
        if first && !letter.eq_ignore_ascii_case(&b'M') {
            return Err(parser.error("path data starts with a move (M or m)"));
        }
        first = false;
        parser.position += 1;
        let relative = letter.is_ascii_lowercase();
        let command = letter.to_ascii_uppercase();
        if command == b'Z' {
            commands.close();
            previous = Previous::Other;
            continue;
        }
        parser.skip_white_space();
        // The first argument group is required; more follow while a number
        // starts after the separator.
        let mut group = 0;
        loop {
            let base = if relative {
                commands.current()
            } else {
                Point::ZERO
            };
            let at = |x: f32, y: f32| Point::new(base.x + x, base.y + y);
            match command {
                b'M' => {
                    let [x, y] = parser.numbers::<2>()?;
                    // A move's later pairs are lines (SVG 2, 9.3.3).
                    if group == 0 {
                        commands.move_to(at(x, y));
                    } else {
                        commands.line_to(at(x, y));
                    }
                    previous = Previous::Other;
                }
                b'L' => {
                    let [x, y] = parser.numbers::<2>()?;
                    commands.line_to(at(x, y));
                    previous = Previous::Other;
                }
                b'H' => {
                    let [x] = parser.numbers::<1>()?;
                    let current = commands.current();
                    commands.line_to(Point::new(base.x + x, current.y));
                    previous = Previous::Other;
                }
                b'V' => {
                    let [y] = parser.numbers::<1>()?;
                    let current = commands.current();
                    commands.line_to(Point::new(current.x, base.y + y));
                    previous = Previous::Other;
                }
                b'C' => {
                    let [x1, y1, x2, y2, x, y] = parser.numbers::<6>()?;
                    let second = at(x2, y2);
                    commands.cubic_to(at(x1, y1), second, at(x, y));
                    previous = Previous::Cubic(second);
                }
                b'S' => {
                    let [x2, y2, x, y] = parser.numbers::<4>()?;
                    let current = commands.current();
                    let first_control = match previous {
                        Previous::Cubic(control) => reflect(control, current),
                        _ => current,
                    };
                    let second = at(x2, y2);
                    commands.cubic_to(first_control, second, at(x, y));
                    previous = Previous::Cubic(second);
                }
                b'Q' => {
                    let [x1, y1, x, y] = parser.numbers::<4>()?;
                    let control = at(x1, y1);
                    commands.quad_to(control, at(x, y));
                    previous = Previous::Quadratic(control);
                }
                b'T' => {
                    let [x, y] = parser.numbers::<2>()?;
                    let current = commands.current();
                    let control = match previous {
                        Previous::Quadratic(control) => reflect(control, current),
                        _ => current,
                    };
                    commands.quad_to(control, at(x, y));
                    previous = Previous::Quadratic(control);
                }
                b'A' => {
                    let ([radius_x, radius_y, rotation], large, sweep, [x, y]) =
                        parser.arc_arguments()?;
                    commands.arc_to(radius_x, radius_y, rotation, large, sweep, at(x, y));
                    previous = Previous::Other;
                }
                _ => unreachable!("every command letter is matched above"),
            }
            group += 1;
            let comma = parser.skip_separator();
            if !parser.at_number() {
                if comma {
                    return Err(parser.error("expected a number after the comma"));
                }
                break;
            }
        }
    }
}

/// `control` reflected about `about`.
fn reflect(control: Point, about: Point) -> Point {
    Point::new(2.0 * about.x - control.x, 2.0 * about.y - control.y)
}
