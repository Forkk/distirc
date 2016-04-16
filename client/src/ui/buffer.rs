//! This module implements the buffer display widget.

use std::rc::Rc;
use std::cell::RefCell;
use std::collections::VecDeque;
use rustbox::{RustBox, Color};

use common::messages::BufferLine;
use common::line::{LineData, MsgKind};

use model::Buffer;
use super::util::RustBoxExt;

#[derive(Debug)]
pub struct BufferView {
    /// The current buffer.
    pub buf: Rc<RefCell<Buffer>>,
    /// When `None`, the view is scrolled to the bottom of the buffer.
    /// Otherwise,this is the number of lines we have scrolled up from the
    /// bottom of the buffer.
    scroll: Option<isize>,
    /// Number of columns reserved for sender timestamps.
    time_col_w: usize,
    /// Number of columns reserved for sender names.
    name_col_w: usize,
}

impl BufferView {
    /// Constructs a new buffer view with the given buffer.
    ///
    /// The view maintains ownership over the buffer during its lifetime.
    /// To get the buffer back, call `into_buf`.
    pub fn new(buf: Rc<RefCell<Buffer>>) -> Self {
        BufferView {
            buf: buf,
            scroll: None,
            time_col_w: 8,
            name_col_w: 10,
        }
    }


    /// Displays the buffer on the terminal.
    ///
    /// The buffer is rendered between rows `y1` and `y2` in the terminal.
    pub fn render(&mut self, rb: &mut RustBox, y1: usize, y2: usize) {
        debug_assert!(y1 < y2);
        debug_assert!(y1 < rb.height());
        let buf = self.buf.borrow();
        let mut y = y2;
        let mut i = self.scroll.unwrap_or(buf.front_len() - 1);
        while y > 0 && i >= -buf.back_len() {
            let ref line = buf.get(i);

            y -= 1;
            i -= 1;
            // let timefmt = line.time.strftime("%H:%M:%S").expect("Failed to format time");
            // let time = format!("{0: >1$}", timefmt, self.time_col_w);
            let time = "TODO";

            match line.data {
                LineData::Message { kind: ref _k, ref from, ref msg } => {
                    let from = format!("<{}>", from);
                    self.render_line(y, rb, time, &from, &msg);
                },
                LineData::Topic { ref by, ref topic } => {
                    let user = by.clone().unwrap_or("*".to_owned());
                    let line = format!("set topic to: {}", topic);
                    self.render_line(y, rb, time, &user, &line);
                },
                LineData::Join { ref user } => {
                    let from = format!("  {0: >1$}", "-->", self.name_col_w);
                    let line = format!("{0} ({1}@{2}) has joined {3}",
                                       user.nick, user.ident, user.host, buf.name());
                    self.render_line(y, rb, time, &from, &line);
                },
                LineData::Part { ref user, ref reason } => {
                    let from = format!("  {0: >1$}", "<--", self.name_col_w);
                    let line = format!("{0} ({1}@{2}) has left {3} ({4})",
                                       user.nick, user.ident, user.host, buf.name(), reason);
                    self.render_line(y, rb, time, &from, &line);
                },
                LineData::Quit { ref user, ref msg } => {
                    let from = format!("  {0: >1$}", "<--", self.name_col_w);
                    let line = format!("{0} ({1}@{2}) has quit ({3})",
                                       user.nick, user.ident, user.host, msg);
                    self.render_line(y, rb, time, &from, &line);
                },
                LineData::Kick { ref by, ref user, ref reason } => {
                    let from = format!("  {0: >1$}", "<--", self.name_col_w);
                    let line = format!("{} was kicked by {} ({})", user, by.nick, reason);
                    self.render_line(y, rb, time, &from, &line);
                },
            }
        }
    }

    fn render_line(&self, y: usize, rb: &mut RustBox, time: &str, from: &str, line: &str) {
        use rustbox::{RB_NORMAL, RB_BOLD};
        use rustbox::Color::*;
        use super::util::AlignCol::*;

        rb.print_cols(y) // style, fgcolor, bgcolor
            .print_col_w(RB_NORMAL, Default, Default, Right(self.time_col_w), time)
            .print_col(RB_NORMAL, Default, Default, " | ")
            .print_col_w(RB_BOLD, Default, Default, Left(self.name_col_w), from)
            .print_col(RB_NORMAL, Default, Default, line);
    }


    /// Scrolls by the given number of lines. Negative is up.
    pub fn scroll_by(&mut self, by: isize) {
        let buf = self.buf.borrow();
        let new = self.scroll.unwrap_or(0) + by;
        if new > buf.front_len() {
            self.scroll = None;
        } else if new < -buf.back_len() {
            self.scroll = Some(-buf.back_len());
        } else {
            self.scroll = Some(new);
        }
    }
}
