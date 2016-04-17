use std::time::Duration;
use rustbox;
use rustbox::{ RustBox, Event, Key };

use model::{CoreModel, Buffer, BufKey};
use conn::ConnThread;

mod buffer;
mod entry;
mod bar;
mod util;

use self::entry::TextEntry;
use self::buffer::BufferView;
use self::bar::{StatusBar, MainBar};

/// Stores the terminal UI's state.
pub struct TermUi {
    rb: RustBox,
    pub entry: TextEntry,
    pub view: BufferView,
    key: BufKey,
    model: CoreModel,
    quit: bool,
}

impl TermUi {
    pub fn new(status: Buffer, conn: ConnThread) -> Result<TermUi, rustbox::InitError> {
        let rb = try!(RustBox::init(rustbox::InitOptions {
            input_mode: rustbox::InputMode::Current,
            buffer_stderr: true,
        }));

        let model = CoreModel::new(status, conn);

        let key = (None, None);
        let buf = model.get(&key).unwrap().clone();

        Ok(TermUi {
            rb: rb,
            entry: TextEntry::new(),
            view: BufferView::new(buf),
            key: key,
            model: model,
            quit: false,
        })
    }

    /// The main function. Runs the client.
    pub fn main(&mut self) {
        // Status bars below the buffer.
        let mut upper_bars: Vec<Box<StatusBar>> = vec![
        ];
        // Status bars above the buffer.
        let mut lower_bars: Vec<Box<StatusBar>> = vec![
            Box::new(MainBar) as Box<StatusBar>,
        ];

        'main: loop {
            self.model.update();

            for bar in upper_bars.iter_mut() { bar.update(self); }
            for bar in lower_bars.iter_mut() { bar.update(self); }
            self.render(&mut upper_bars, &mut lower_bars);

            if let Ok(e) = self.rb.peek_event(Duration::from_millis(200), false) {
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
            }

            if self.quit { break 'main; }
        }
    }


    /// Handles something typed into the text entry.
    pub fn handle_input(&mut self, line: String) {
        trace!("Typed: {}", &line);
        if line.starts_with("/") {
            let line = &line[1..];
            if let Some(spc) = line.find(' ') {
                self.handle_command(&line[..spc], &line[spc + 1..])
            } else {
                self.handle_command(line, &"");
            }
        } else {
            self.model.send_privmsg(&self.key, line);
        }
    }

    pub fn handle_command(&mut self, cmd: &str, args: &str) {
        match cmd {
            "quit" => { self.quit = true; },
            "s" | "switch" => {
                if args == "" {
                    self.switch_buf((None, None));
                } else {
                    self.switch_buf((Some("esper".to_owned()), Some(args.to_owned())));
                }
            },
            "j" | "join" => {
                let args = args.split(' ').collect::<Vec<_>>();
                if args.len() == 2 {
                    self.model.send_join(args[0].to_owned(), args[1].to_owned());
                } else {
                    // TODO: Implement error reporting system.
                    warn!("Usage: /join [network] [channel]");
                }
            },
            "p" | "part" => {
                let args = args.split(' ').collect::<Vec<_>>();
                if args.len() >= 2 {
                    self.model.send_part(args[0].to_owned(), args[1].to_owned(), args[1..].join(" "));
                } else {
                    // TODO: Implement error reporting system.
                    warn!("Usage: /part [network] [channel] [message..]");
                }
            },
            _ => { warn!("Unrecognized command: {}", cmd); },
        }
    }


    /// Switches to the buffer with the given key.
    pub fn switch_buf(&mut self, key: BufKey) {
        if let Some(ref mut buf) = self.model.get(&key) {
            info!("Switched buffer to {:?}", key);
            self.key = key;
            self.view = BufferView::new(buf.clone());
        } else {
            error!("No such buffer: {:?}", key);
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
            Key::PageUp => self.view.scroll_by(-10),
            Key::PageDown => self.view.scroll_by(10),
            _ => {},
        }
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
