//! This module implements the buffer display widget.

use std::rc::Rc;
use std::cell::RefCell;
use std::collections::VecDeque;
use rustbox::{RustBox, Color};

use model::{BufHandle, BufferLine};

#[derive(Debug)]
pub struct Buffer {
    /// The name of the buffer.
    name: String,
    /// Handle to the current buffer.
    handle: Rc<RefCell<BufHandle>>,
    lines: VecDeque<BufferLine>,
    /// When `None`, the view is scrolled to the bottom of the buffer.
    /// Otherwise,this is the number of lines we have scrolled up from the
    /// bottom of the buffer.
    scroll: Option<usize>,
    /// Number of columns reserved for sender timestamps.
    time_col_w: usize,
    /// Number of columns reserved for sender names.
    name_col_w: usize,
}

impl Buffer {
    /// Constructs a new buffer view with the given buffer.
    ///
    /// The view maintains ownership over the buffer during its lifetime.
    /// To get the buffer back, call `into_buf`.
    pub fn new(name: &str, hand: Rc<RefCell<BufHandle>>) -> Buffer {
        Buffer {
            name: name.to_owned(),
            handle: hand,
            lines: VecDeque::new(),
            scroll: None,
            time_col_w: 8,
            name_col_w: 10,
        }
    }


    /// Updates the contents of the view with any new lines in the given buffer.
    pub fn update(&mut self) {
        let mut h = self.handle.borrow_mut();
        let front = h.take_new_front();
        let back = h.take_new_back();

        let front_len = front.len();
        for line in front.into_iter() {
            self.lines.push_front(line);
        }
        // If we're not at the bottom, scroll up by the number of newly added
        // lines.
        if let Some(ref mut s) = self.scroll {
            *s += front_len;
        }

        for line in back.into_iter() {
            self.lines.push_back(line);
        }
    }


    /// Displays the buffer on the terminal.
    ///
    /// The buffer is rendered between rows `y1` and `y2` in the terminal.
    pub fn render(&mut self, rb: &mut RustBox, y1: usize, y2: usize) {
        debug_assert!(y1 < y2);
        debug_assert!(y1 < rb.height());
        let mut y = y2;
        let mut i = self.scroll.unwrap_or(0);
        while y > 0 && i < self.lines.len() {
            use rustbox::{RB_NORMAL, RB_BOLD};

            let ref line = self.lines[i];

            y -= 1;
            i += 1;
            let timefmt = line.time.strftime("%H:%M:%S").expect("Failed to format time");
            let time = format!("{0: >1$}", timefmt, self.time_col_w);
            let name = format!("<{0: >1$}>:", line.from, self.name_col_w);
            let ref text = line.text;

            let mut x = 0;
            rb.print(x, y, RB_NORMAL, Color::Default, Color::Default, &time);
            x += time.len() + 1;
            rb.print_char(x, y, RB_NORMAL, Color::Default, Color::Default, '|');
            x += 2;
            rb.print(x, y, RB_BOLD, Color::Default, Color::Default, &name);
            x += name.len() + 1;
            rb.print(x, y, RB_NORMAL, Color::Default, Color::Default, &text);
        }
    }


    /// Scrolls by the given number of lines. Positive is up.
    pub fn scroll_by(&mut self, by: isize) {
        let new = self.scroll.unwrap_or(0) as isize + by;
        if new >= self.lines.len() as isize {
            self.scroll = Some(self.lines.len() as usize - 1);
        } else if new <= 0 {
            self.scroll = None;
        } else {
            self.scroll = Some(new as usize);
        }
    }


    pub fn get_name(&self) -> &str {
        &self.name
    }
}
