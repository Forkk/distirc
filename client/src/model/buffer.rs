use std::sync::Arc;
use std::sync::mpsc::{channel, Sender, Receiver};
use std::sync::atomic::{AtomicBool, Ordering};
use common::messages::BufferLine;

use time::{Tm, now};


/// Sends lines to a `Buffer` in a thread-safe manner.
#[derive(Debug)]
pub struct BufSender {
    front: Sender<BufferLine>,
    back: Sender<BufferLine>,
    log_req: Arc<AtomicBool>,
}

impl BufSender {
    pub fn send_front(&mut self, line: BufferLine) {
        self.front.send(line)
            .expect("Sender's buffer was dropped")
    }

    pub fn send_back(&mut self, line: BufferLine) {
        self.back.send(line)
            .expect("Sender's buffer was dropped")
    }
}


/// A buffer that receives lines from a paired sender.
///
/// Buffers store lines using an indexing system with negative indices. The
/// largest index represents the most recently received message. Index 0
/// represents the first message in the `front` buffer, a list of messages that
/// have been received since the buffer was constructed. Starting from -1,
/// indices below 0 represent lines that were received from the server as
/// scrollback.
///
/// This system allows messages to be appended to both ends of the buffer in a
/// way that is fast and doesn't change the indices of existing messages.
#[derive(Debug)]
pub struct Buffer {
    name: String,
    front_rx: Receiver<BufferLine>,
    back_rx: Receiver<BufferLine>,
    /// When this set to true, the client will ask the server for more backlogs
    /// if applicable.
    log_req: Arc<AtomicBool>,
    /// Lines added since the buffer was connected.
    front: Vec<BufferLine>,
    /// Scrollback lines in reverse order. The first of these is at index -1.
    back: Vec<BufferLine>,
}

impl Buffer {
    /// Creates a new buffer, sender pair.
    pub fn new(name: String) -> (Buffer, BufSender) {
        let (tx1, rx1) = channel();
        let (tx2, rx2) = channel();
        let log_req = Arc::new(AtomicBool::new(false));

        let sender = BufSender {
            front: tx1,
            back: tx2,
            log_req: log_req.clone(),
        };
        let buf = Buffer {
            name: name,
            front_rx: rx1,
            back_rx: rx2,
            log_req: log_req,
            front: vec![],
            back: vec![],
        };
        (buf, sender)
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// Receives new messages from the sender.
    pub fn update(&mut self) {
        while let Ok(line) = self.front_rx.try_recv() {
            self.front.push(line)
        }
        while let Ok(line) = self.back_rx.try_recv() {
            self.back.push(line)
        }
    }

    pub fn get(&self, idx: isize) -> &BufferLine {
        if idx < 0 {
            &self.back[(-idx) as usize - 1]
        } else { &self.front[idx as usize] }
    }

    /// Returns the length of the front buffer. This is the index of the most
    /// recently received message + 1.
    pub fn front_len(&self) -> isize {
        self.front.len() as isize
    }

    /// Returns the length of the back buffer. This is the negative of the index
    /// of the oldest message.
    pub fn back_len(&self) -> isize {
        self.back.len() as isize
    }

    /// Tells the client to request more backlogs from the server.
    pub fn request_logs(&mut self) {
        self.log_req.store(true, Ordering::Relaxed);
    }

    // /// Pushes a status message into the buffer.
    // pub fn push_status(&mut self, msg: &str) {
    //     self.last_id += 1;
    //     self.front_lines.push(BufferLine{
    //         id: self.last_id,
    //         // time: now(),
    //         data: LineData::Message {
    //             from: "status".to_owned(),
    //             msg: msg.to_owned(),
    //             kind: MsgKind::Status,
    //         },
    //     });
    // }
}
