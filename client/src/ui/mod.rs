use rustbox;
use rustbox::{ RustBox, Event, Key };


mod entry;

use self::entry::TextEntry;

/// Stores the terminal UI's state.
pub struct TermUi {
    rb: RustBox,
    entry: TextEntry,
    quit: bool,
}

impl TermUi {
    pub fn new() -> Result<TermUi, rustbox::InitError> {
        let rb = try!(RustBox::init(rustbox::InitOptions {
            input_mode: rustbox::InputMode::Current,
            buffer_stderr: true,
        }));
        let entry = TextEntry::new();
        Ok(TermUi {
            rb: rb,
            entry: entry,
            quit: false,
        })
    }

    /// The main function. Runs the client.
    pub fn main(&mut self) {
        self.init();

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

            self.render();
        }
    }

    /// Handles something typed into the text entry.
    pub fn handle_input(&mut self, line: String) {
        if line == "/quit" {
            self.quit = true;
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
            _ => {},
        }
    }

    /// Initializes UI widgets.
    fn init(&mut self) {
        self.render();
    }

    fn render(&mut self) {
        self.rb.clear();

        self.entry.render(&mut self.rb);

        self.rb.present();
    }
}
