//! Utilities for rendering to the terminal

use rustbox::{RustBox, Color, Style};

/// Extension trait for `RustBox` utils.
pub trait RustBoxExt : Sized {
    /// Checks how wide the given text would be in the terminal.
    fn text_width(&self, s: &str) -> usize;

    /// Constructs a `ColumnRender` object for aligning columns of text.
    fn print_cols<'a>(&'a mut self, y: usize) -> ColumnRender<'a>;

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

    fn print_cols<'a>(&'a mut self, y: usize) -> ColumnRender<'a> {
        ColumnRender {
            rb: self,
            x: 0,
            y: y,
        }
    }

    fn blank_line(&mut self, y: usize, style: Style, fgcolor: Color, bgcolor: Color) {
        let w = self.width();
        // Generate 'w' many blank spaces
        let blank = String::from_utf8(vec![b' '; w]).unwrap();
        self.print(0, y, style, fgcolor, bgcolor, &blank);
    }
}


/// Builder pattern type for aligning text in columns.
pub struct ColumnRender<'a> {
    rb: &'a mut RustBox,
    x: usize,
    y: usize,
}

pub enum AlignCol{
    /// left-align with given width
    Left(usize),
    /// right-align with given width
    Right(usize),
}

impl<'a> ColumnRender<'a> {
    pub fn print_col(mut self, style: Style, fgcolor: Color, bgcolor: Color, text: &str) -> Self {
        self.rb.print(self.x, self.y, style, fgcolor, bgcolor, text);
        self.x += self.rb.text_width(text);
        self
    }

    /// Prints a column with a given width and alignment.
    pub fn print_col_w(self, style: Style, fgcolor: Color, bgcolor: Color, pad: AlignCol, text: &str) -> Self {
        use self::AlignCol::*;
        let text = match pad {
            Left(w)  => { format!("{0: <1$}", text, w) },
            Right(w) => { format!("{0: >1$}", text, w) },
        };
        self.print_col(style, fgcolor, bgcolor, &text)
    }

    /// Skips `n` columns
    pub fn skip(mut self, n: usize) -> Self {
        self.x += n;
        self
    }
}
