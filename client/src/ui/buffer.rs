//! This module implements the buffer display widget.

use std::rc::Rc;
use std::cell::RefCell;
use rustbox::RustBox;

use common::line::{LineData, MsgKind};

use model::Buffer;

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
    pub fn new(bh: Rc<RefCell<Buffer>>, rb: &mut RustBox) -> Self {
        {
            let mut buf = bh.borrow_mut();
            if rb.height() > buf.len() {
                buf.request_logs(rb.height());
            }
        }
        BufferView {
            buf: bh,
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

            i -= 1;
            let tm = line.time();
            let timefmt = tm.strftime("%H:%M:%S").expect("Failed to format time");
            let time = format!("{0: >1$}", timefmt, self.time_col_w);

            let dy = match line.data {
                LineData::Message { ref kind, ref from, ref msg, .. } => {
                    let (from, msg) = match *kind {
                        MsgKind::PrivMsg =>
                            (format!("<{}>", from), msg.to_owned()),
                        MsgKind::Notice =>
                            (format!("[{}]", from), msg.to_owned()),
                        MsgKind::Action =>
                            (format!(" * "), format!("{} {}", from, msg)),
                        MsgKind::Response(_) =>
                            (format!("{}", from), msg.to_owned()),
                        MsgKind::Status =>
                            (format!("*{}*", from), msg.to_owned()),
                    };
                    self.render_line(y, rb, &time, &from, &msg)
                },
                LineData::Topic { ref by, ref topic } => {
                    let user = by.clone().unwrap_or("*".to_owned());
                    let line = format!("set topic to: {}", topic);
                    self.render_line(y, rb, &time, &user, &line)
                },
                LineData::Join { ref user } => {
                    let line = format!("{0} ({1}@{2}) has joined {3}",
                                       user.nick, user.ident, user.host, buf.name());
                    // let line = format!("{0} has joined {1}",
                    //                    user.nick, buf.name());
                    self.render_line(y, rb, &time, "-->", &line)
                },
                LineData::Part { ref user, ref reason } => {
                    let line = format!("{0} ({1}@{2}) has left {3} ({4})",
                                       user.nick, user.ident, user.host, buf.name(), reason);
                    // let line = format!("{0} has left {1} ({2})",
                    //                    user.nick, user.ident, reason);
                    self.render_line(y, rb, &time, "<--", &line)
                },
                LineData::Quit { ref user, ref msg } => {
                    let msg = msg.clone().unwrap_or("No message".to_owned());
                    let line = format!("{0} ({1}@{2}) has quit ({3})",
                                       user.nick, user.ident, user.host, msg);
                    // let line = format!("{0} has quit ({1})",
                    //                    user.nick, msg);
                    self.render_line(y, rb, &time, "<--", &line)
                },
                LineData::Kick { ref by, ref user, ref reason } => {
                    let line = format!("{} was kicked by {} ({})", user, by.nick, reason);
                    self.render_line(y, rb, &time, "<--", &line)
                },
                LineData::Nick { ref user, ref new } => {
                    let line = format!("{} is now known as {}", user, new);
                    self.render_line(y, rb, &time, "***", &line)
                },
            };
            if y > dy {
                y -= dy;
            } else { break; }
        }
    }

    fn render_line(&self, mut y: usize, rb: &mut RustBox, time: &str, from: &str, line: &str) -> usize {
        use rustbox::RB_BOLD;
        use super::util::LineBuilder;

        let mut lb = LineBuilder::new();

        lb.skip(1);
        lb.add_column(time.to_owned())
            .pad_right(self.time_col_w);
        lb.skip(1);
        lb.add_column(from.to_owned())
            .style(RB_BOLD)
            .pad_left(self.name_col_w);
        lb.skip(1);
        lb.add_column(line.to_owned())
            .wrap();

        let h = lb.height(rb);
        if y > h {
            y -= h;
            lb.print(y, rb);
        }
        h
    }


    /// Scrolls by the given number of lines and fetches backlog from the server
    /// if we've scrolled to the top.
    pub fn scroll_and_fetch(&mut self, by: isize, rb: &mut RustBox) {
        {
            let mut buf = self.buf.borrow_mut();
            let start = if buf.is_empty() {
                0
            } else {
                buf.first_idx()
            };
            let new = self.scroll.unwrap_or(start) + by;
            let last = buf.last_idx();
            if new - (rb.height() as isize) < last {
                debug!("Fetching more logs. Last: {}", last);
                buf.request_logs(50);
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

    /// Returns the number of lines we've scrolled up from the bottom.
    pub fn scroll_height(&self) -> usize {
        match self.scroll {
            Some(line) => (-line - self.buf.borrow().first_idx()) as usize,
            None => 0,
        }
    }
}
