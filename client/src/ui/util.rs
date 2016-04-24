//! Utilities for rendering to the terminal

use std::cmp;
use rustbox::{RustBox, Color, Style};

use super::wrap::StringWrap;

/// Extension trait for `RustBox` utils.
pub trait RustBoxExt : Sized {
    /// Checks how wide the given text would be in the terminal.
    fn text_width(&self, s: &str) -> usize;

    /// Renders blank spaces of the given style across the line at `y`.
    fn blank_line(&mut self, y: usize, style: Style, fgcolor: Color, bgcolor: Color);
}

impl RustBoxExt for RustBox {
    fn text_width(&self, s: &str) -> usize {
        // FIXME: This isn't even remotely accurate, especially not for
        // multi-byte unicode chars, but RustBox doesn't handle those correctly
        // anyway, so we won't worry about it for now.
        s.len()
    }

    fn blank_line(&mut self, y: usize, style: Style, fgcolor: Color, bgcolor: Color) {
        let w = self.width();
        // Generate 'w' many blank spaces
        let blank = String::from_utf8(vec![b' '; w]).unwrap();
        self.print(0, y, style, fgcolor, bgcolor, &blank);
    }
}


/// Builder pattern type for rendering with columns and line wrapping.
pub struct LineBuilder {
    cols: Vec<ColBuilder>,
}

impl LineBuilder {
    pub fn new() -> LineBuilder {
        LineBuilder {
            cols: vec![],
        }
    }

    /// Adds a column and returns a `&mut` reference to it.
    pub fn add_column(&mut self, text: String) -> &mut ColBuilder {
        use rustbox::RB_NORMAL;
        self.cols.push(ColBuilder {
            text: text,
            wrap: false,
            wrapping: None,
            pad: None,
            style: RB_NORMAL,
            fgcolor: Color::Default,
            bgcolor: Color::Default,
        });
        let idx = self.cols.len()-1;
        &mut self.cols[idx]
    }

    /// Skips a space of the given width.
    pub fn skip(&mut self, w: usize) {
        self.add_column(String::new()).pad_right(w);
    }


    /// Calculates the line's height after line wrapping in the given terminal.
    pub fn height(&mut self, rb: &mut RustBox) -> usize {
        let term_w = rb.width();
        let mut x = 0;

        let mut max_h = 1;
        for mut col in self.cols.iter_mut() {
            let w = if x < term_w {
                col.pad.clone().map(|p| p.width()).unwrap_or(term_w - x)
            } else { 0 };

            if col.wrap {
                let wrap = col.wrap_to(w);
                let h = wrap.line_count();
                max_h = cmp::max(h, max_h);
            }

            x += w;
            if x > term_w { break; }
        }
        max_h
    }

    /// Prints this line in the given terminal.
    pub fn print(self, y: usize, rb: &mut RustBox) {
        use self::PadText::*;

        let term_w = rb.width();
        let mut x = 0;
        for mut col in self.cols {
            let w = if x < term_w {
                col.pad.clone().map(|p| p.width()).unwrap_or(term_w - x)
            } else { 0 };

            if col.wrap {
                let wrap = col.wrap_to(w);
                for (i, line) in wrap.iter_lines(&col.text).enumerate() {
                    rb.print(x, y + i - 1, col.style, col.fgcolor, col.bgcolor, line);
                }
            } else {
                let text = match col.pad {
                    Some(Left(w))  => { format!("{0: >1$}", col.text, w) },
                    Some(Right(w)) => { format!("{0: <1$}", col.text, w) },
                    None => col.text,
                };
                rb.print(x, y, col.style, col.fgcolor, col.bgcolor, &text);
            }
            x += w;
        }
    }
}


/// Column within a `LineBuilder`.
#[derive(Debug, Clone)]
pub struct ColBuilder {
    text: String,
    /// If true, text will be wrapped onto subsequent lines.
    wrap: bool,
    wrapping: Option<StringWrap>,
    /// The column's width and padding. If `None`, the column will expand to
    /// fill the entire remaining width of the screen.
    pad: Option<PadText>,

    style: Style,
    fgcolor: Color,
    bgcolor: Color,
}

impl ColBuilder {
    /// Enables word wrapping for this column.
    pub fn wrap(&mut self) -> &mut Self {
        self.wrap = true;
        self
    }

    /// Left-pads text to the given width.
    pub fn pad_left(&mut self, w: usize) -> &mut Self {
        self.pad = Some(PadText::Left(w));
        self
    }

    /// Right-pads text to the given width.
    pub fn pad_right(&mut self, w: usize) -> &mut Self {
        self.pad = Some(PadText::Right(w));
        self
    }

    /// Sets the style for text in this column.
    pub fn style(&mut self, style: Style) -> &mut Self {
        self.style = style;
        self
    }

    /// Sets the fgcolor for text in this column.
    pub fn fgcolor(&mut self, color: Color) -> &mut Self {
        self.fgcolor = color;
        self
    }

    /// Sets the bgcolor for text in this column.
    pub fn bgcolor(&mut self, color: Color) -> &mut Self {
        self.bgcolor = color;
        self
    }
}

impl ColBuilder {
    /// Calculates line wrapping for this column.
    fn wrap_to(&mut self, width: usize) -> StringWrap {
        StringWrap::new(&self.text, width)
    }
}


#[derive(Debug, Clone)]
pub enum PadText {
    /// left-pad with given width
    Left(usize),
    /// right-pad with given width
    Right(usize),
}

impl PadText {
    pub fn width(&self) -> usize {
        match *self {
            PadText::Left(w) => w,
            PadText::Right(w) => w,
        }
    }
}
