use std::collections::{VecDeque, HashMap};
use std::rc::Rc;
use std::cell::RefCell;
use rustbox;
use rustbox::{ RustBox, Event, Key };

use common::messages::{
    BufTarget, BufId, NetId,
    CoreMsg, CoreBufMsg, CoreNetMsg,
    ClientMsg,
};

use model::BufHandle;
use conn::ConnThread;


mod buffer;
mod entry;
mod bar;
mod util;

use self::entry::TextEntry;
use self::buffer::{ Buffer };
use self::bar::{ StatusBar, MainBar };

/// Stores the terminal UI's state.
pub struct TermUi {
    rb: RustBox,
    pub entry: TextEntry,
    pub view: Buffer,
    // In the keys here, when the `NetId` is `None`, the value is a global
    // buffer. When the `BufId` is none and the `NetId` isn't, the buffer is the
    // network's status buffer. When both are none, it is the client status
    // buffer.
    bufs: HashMap<(Option<NetId>, Option<BufId>), Rc<RefCell<BufHandle>>>,
    sendq: VecDeque<ClientMsg>,
    quit: bool,
}

impl TermUi {
    pub fn new() -> Result<TermUi, rustbox::InitError> {
        let rb = try!(RustBox::init(rustbox::InitOptions {
            input_mode: rustbox::InputMode::Current,
            buffer_stderr: true,
        }));

        let mut bh = BufHandle::new();
        bh.push_status("Welcome to distirc's terminal UI!");
        let bh = Rc::new(RefCell::new(bh));

        let mut bufs = HashMap::new();
        bufs.insert((None, None), bh.clone());

        let entry = TextEntry::new();
        Ok(TermUi {
            rb: rb,
            entry: entry,
            view: Buffer::new("*status*", bh.clone()),
            bufs: bufs,
            sendq: VecDeque::new(),
            quit: false,
        })
    }

    /// The main function. Runs the client.
    pub fn main(&mut self, mut conn: ConnThread) {
        // Status bars below the buffer.
        let mut upper_bars: Vec<Box<StatusBar>> = vec![
        ];
        // Status bars above the buffer.
        let mut lower_bars: Vec<Box<StatusBar>> = vec![
            Box::new(MainBar) as Box<StatusBar>,
        ];

        self.view.update();
        self.render(&mut upper_bars, &mut lower_bars);

        'main: while !self.quit {
            while let Some(msg) = conn.recv() {
                self.handle_msg(msg);
            }

            // TODO: Don't crash when this fails.
            let e = self.rb.poll_event(false).expect("Failed to get event");

            if let Event::KeyEvent(Key::Ctrl('c')) = e {
                break 'main;
            }

            if !self.entry.handle(&e) {
                self.handle_event(&e);
            } else {
                if let Some(line) = self.entry.next_entry() {
                    self.handle_input(line);
                }
            }

            for bar in upper_bars.iter_mut() { bar.update(self); }
            for bar in lower_bars.iter_mut() { bar.update(self); }

            self.view.update();
            self.render(&mut upper_bars, &mut lower_bars);
        }
    }


    /// Handles something typed into the text entry.
    pub fn handle_input(&mut self, line: String) {
        if line == "/quit" {
            self.quit = true;
        } else {
            // TODO
            // self.status_bh.borrow_mut().push_status(&line);
        }
    }

    pub fn handle_event(&mut self, evt: &Event) {
        match *evt {
            Event::KeyEvent(key) => self.handle_key(&key),
            _ => {},
        }
    }

    pub fn handle_key(&mut self, key: &Key) {
        match *key {
            Key::PageUp => self.view.scroll_by(10),
            Key::PageDown => self.view.scroll_by(-10),
            _ => {},
        }
    }


    pub fn handle_msg(&mut self, msg: CoreMsg) {
        match msg {
            CoreMsg::Networks(nets) => {
                debug!("New networks: {:?}", nets);
            },
            CoreMsg::GlobalBufs(bufs) => {
                debug!("New global buffers: {:?}", bufs);
                for buf in bufs {
                    let key = (None, Some(buf.name));
                    if !self.bufs.contains_key(&key) {
                        let bh = BufHandle::new();
                        let bh = Rc::new(RefCell::new(bh));
                        self.bufs.insert(key, bh);
                    }
                }
            },
            CoreMsg::NetMsg(nid, nmsg) => self.handle_net_msg(nid, nmsg),
            CoreMsg::BufMsg(bid, bmsg) => self.handle_buf_msg((None, Some(bid)), bmsg),
        }
    }

    pub fn handle_net_msg(&mut self, net: NetId, msg: CoreNetMsg) {
        match msg {
            CoreNetMsg::State { connected } => {
                if connected {
                    info!("Core connected to network {}", net);
                } else {
                    info!("Core disconnected from network {}", net);
                }
            },
            CoreNetMsg::Buffers(bufs) => {
                debug!("New buffers for network {}: {:?}", net, bufs);
                for buf in bufs {
                    let key = (Some(net.clone()), Some(buf.name));
                    if !self.bufs.contains_key(&key) {
                        let bh = BufHandle::new();
                        let bh = Rc::new(RefCell::new(bh));
                        self.bufs.insert(key, bh);
                    }
                }
            },
            CoreNetMsg::BufMsg(BufTarget::Network, bmsg) => self.handle_buf_msg((Some(net), None), bmsg),
            CoreNetMsg::BufMsg(BufTarget::Channel(buf), bmsg) =>
                self.handle_buf_msg((Some(net), Some(buf)), bmsg),
            CoreNetMsg::BufMsg(BufTarget::Private(buf), bmsg) =>
                self.handle_buf_msg((Some(net), Some(buf)), bmsg),
        }
    }

    pub fn handle_buf_msg(&mut self, key: (Option<NetId>, Option<BufId>), msg: CoreBufMsg) {
        let bh = match self.bufs.get_mut(&key) {
            Some(b) => b,
            None => {
                error!("Ignoring message for unknown buffer: {:?}", key);
                return;
            },
        };

        match msg {
            CoreBufMsg::State { joined } => {
                if joined {
                    info!("Joined channel {}", key.1.unwrap_or("*status*".to_owned()));
                } else {
                    info!("Parted channel {}", key.1.unwrap_or("*status*".to_owned()));
                }
            },
            CoreBufMsg::NewLines(lines) => {
                bh.borrow_mut().push_lines_front(lines);
            },
            CoreBufMsg::Scrollback(lines) => {
                bh.borrow_mut().push_lines_back(lines);
            },
        }
    }


    fn send(&mut self, msg: ClientMsg) {
        self.sendq.push_back(msg);
    }


    /// Renders the UI.
    ///
    /// `btop` and `bbot` are the status bars on the top and bottom
    /// of the buffer view.
    fn render(&mut self, btop: &mut Vec<Box<StatusBar>>, bbot: &mut Vec<Box<StatusBar>>) {
        self.rb.clear();

        self.entry.render(&mut self.rb);

        let y1 = btop.len();
        let y2 = self.rb.height() - 1 - bbot.len();
        self.view.render(&mut self.rb, y1, y2);

        for (y, bar) in btop.iter_mut().enumerate() {
            bar.render(y, self);
        }
        let h = self.rb.height();
        for (y, bar) in bbot.iter_mut().rev().enumerate() {
            bar.render(h - 2 - y, self);
        }

        self.rb.present();
    }
}
