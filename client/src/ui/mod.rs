use std::rc::Rc;
use std::cell::RefCell;
use rustbox;
use rustbox::{ RustBox, Event, Key };

use model::BufHandle;


mod buffer;
mod entry;
mod bar;

use self::entry::TextEntry;
use self::buffer::{ Buffer };
use self::bar::{ StatusBar, MainBar };

/// Stores the terminal UI's state.
pub struct TermUi {
    rb: RustBox,
    pub entry: TextEntry,
    pub view: Buffer,
    status_bh: Rc<RefCell<BufHandle>>,
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

        let entry = TextEntry::new();
        Ok(TermUi {
            rb: rb,
            entry: entry,
            view: Buffer::new("*status*", bh.clone()),
            status_bh: bh,
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

        self.view.update();
        self.render(&mut upper_bars, &mut lower_bars);

        'main: while !self.quit {
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
            self.status_bh.borrow_mut().push_status(&line);
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
