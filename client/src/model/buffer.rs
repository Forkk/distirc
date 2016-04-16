use common::messages::BufferLine;
use common::line::{LineData, MsgKind};

use time::{Tm, now};


/// Represents a handle to a buffer.
///
/// This object keeps track of lines that have been added to either end of the
/// buffer but haven't yet been processed by the view.
#[derive(Debug)]
pub struct BufHandle {
    front_lines: Vec<BufferLine>,
    back_lines: Vec<BufferLine>,
    /// When this set to true, the client will ask the server for more backlogs.
    fetch_logs: bool,
    last_id: usize,
}

impl BufHandle {
    pub fn new() -> BufHandle {
        BufHandle {
            front_lines: vec![],
            back_lines: vec![],
            fetch_logs: false,
            last_id: 0,
        }
    }

    /// Pushes a status message into the buffer.
    pub fn push_status(&mut self, msg: &str) {
        self.last_id += 1;
        self.front_lines.push(BufferLine{
            id: self.last_id,
            // time: now(),
            data: LineData::Message {
                from: "status".to_owned(),
                msg: msg.to_owned(),
                kind: MsgKind::Status,
            },
        });
    }


    /// Pushes new lines into the front buffer.
    pub fn push_lines_front(&mut self, mut lines: Vec<BufferLine>) {
        self.front_lines.append(&mut lines);
    }

    /// Pushes new lines into the back of the buffer.
    pub fn push_lines_back(&mut self, mut lines: Vec<BufferLine>) {
        self.back_lines.append(&mut lines);
    }


    /// Takes all newly received front lines.
    pub fn take_new_front(&mut self) -> Vec<BufferLine> {
        use std::mem;
        let mut front = vec![];
        mem::swap(&mut front, &mut self.front_lines);
        front
    }

    /// Takes all newly received back lines.
    pub fn take_new_back(&mut self) -> Vec<BufferLine> {
        use std::mem;
        let mut back = vec![];
        mem::swap(&mut back, &mut self.back_lines);
        back
    }

    /// Tells the client to request more backlogs from the server.
    pub fn request_logs(&mut self) {
        self.fetch_logs = true;
    }
}
