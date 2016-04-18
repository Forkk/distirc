//! This module implements the buffer display widget.

use std::rc::Rc;
use std::cell::RefCell;
use rustbox::RustBox;

use common::line::LineData;

use model::Buffer;
use super::util::RustBoxExt;

#[derive(Debug)]
pub struct BufferView {
    /// The current buffer.
    pub buf: Rc<RefCell<Buffer>>,
    /// When `None`, the view is scrolled to the bottom of the buffer.
    /// Otherwise, this is the index of our current line.
    pub scroll: Option<isize>,
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
            name_col_w: 16,
        }
    }


    /// Displays the buffer on the terminal.
    ///
    /// The buffer is rendered between rows `y1` and `y2` in the terminal.
    pub fn render(&mut self, rb: &mut RustBox, y1: usize, y2: usize) {
        debug_assert!(y1 < y2);
        debug_assert!(y1 < rb.height());
        let buf = self.buf.borrow();
        if buf.is_empty() { return; }
        let mut y = y2;
        let mut i = self.scroll.unwrap_or(buf.first_idx());
        while y > y1 && i >= buf.last_idx() {
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
                    let line = format!("{0} ({1}@{2}) has joined {3}",
                                       user.nick, user.ident, user.host, buf.name());
                    self.render_line(y, rb, time, "-->", &line);
                },
                LineData::Part { ref user, ref reason } => {
                    let line = format!("{0} ({1}@{2}) has left {3} ({4})",
                                       user.nick, user.ident, user.host, buf.name(), reason);
                    self.render_line(y, rb, time, "<--", &line);
                },
                LineData::Quit { ref user, ref msg } => {
                    let line = format!("{0} ({1}@{2}) has quit ({3})",
                                       user.nick, user.ident, user.host, msg);
                    self.render_line(y, rb, time, "<--", &line);
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
            .skip(1)
            .print_col_w(RB_NORMAL, Default, Default, Left(self.time_col_w), time)
            .print_col(RB_NORMAL, Default, Default, " | ")
            .print_col_w(RB_BOLD, Default, Default, Right(self.name_col_w), from)
            .skip(1)
            .print_col(RB_NORMAL, Default, Default, line);
    }


    /// Scrolls by the given number of lines and fetches backlog from the server
    /// if we've scrolled to the top.
    pub fn scroll_and_fetch(&mut self, by: isize) {
        {
            let mut buf = self.buf.borrow_mut();
            let start = if buf.is_empty() {
                0
            } else {
                buf.first_idx()
            };
            let new = self.scroll.unwrap_or(start) + by;
            let last = buf.last_idx();
            if new < last {
                debug!("Fetching more logs. Last: {}", last);
                buf.request_logs();
            }
        }
        self.scroll_by(by)
    }


    /// Scrolls by the given number of lines. Negative is up.
    pub fn scroll_by(&mut self, by: isize) {
        let buf = self.buf.borrow();
        if buf.is_empty() {
            self.scroll = None; return;
        }
        let new = self.scroll.unwrap_or(buf.first_idx()) + by;
        if new >= buf.first_idx() {
            self.scroll = None;
        } else if new < buf.last_idx() {
            self.scroll = Some(buf.last_idx());
        } else {
            self.scroll = Some(new);
        }
    }
}
