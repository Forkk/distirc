//! This module implements the text entry widget.

use std::collections::VecDeque;
use rustbox::{ RustBox, Event, Key, Style, Color };

/// The IRC client's text box.
pub struct TextEntry {
    // FIXME: Store cursor position as a pair of terminal column and string index.
    cursor_pos: isize,
    hist: VecDeque<String>,
    hist_pos: usize,
    /// Queue of entries that haven't been processed yet.
    cmds: VecDeque<String>,
}

impl TextEntry {
    pub fn new() -> TextEntry {
        let mut hist = VecDeque::new();
        hist.push_front(String::new());
        TextEntry {
            cursor_pos: 0,
            hist: hist,
            hist_pos: 0,
            cmds: VecDeque::new(),
        }
    }

    /// Returns the next entry in the list of things the user has typed.
    pub fn next_entry(&mut self) -> Option<String> {
        self.cmds.pop_front()
    }

    pub fn render(&self, rb: &mut RustBox) {
        debug_assert!(self.cursor_pos >= 0);
        debug_assert!(self.cursor_pos <= self.get_text().len() as isize);

        let h = rb.height();
        rb.print(0, h - 1, Style::empty(), Color::Default, Color::Default, self.get_text());
        rb.set_cursor(self.cursor_col(), h as isize - 1);
    }

    /// Handles an event
    ///
    /// Returns true if the event was handled.
    pub fn handle(&mut self, evt: &Event) -> bool {
        let ret = match *evt {
            Event::KeyEvent(key) => self.handle_key(&key),
            _ => false,
        };
        debug_assert!(self.cursor_pos >= 0);
        debug_assert!(self.cursor_pos <= self.get_text().len() as isize);
        ret
    }

    fn handle_key(&mut self, key: &Key) -> bool {
        match *key {
            Key::Char(ch) => {
                // FIXME: This will break on multi-byte unicode characters.
                self.hist[self.hist_pos].insert(self.cursor_pos as usize, ch);
                self.move_cursor(|p| p + 1);
                true
            },
            Key::Backspace => {
                if self.cursor_pos > 0 {
                    self.hist[self.hist_pos].remove(self.cursor_pos as usize - 1);
                    self.move_cursor(|p| p - 1);
                }
                true
            },
            Key::Left => { self.move_cursor(|p| p - 1); true },
            Key::Right => { self.move_cursor(|p| p + 1); true },
            Key::Home => { self.move_cursor(|_| 0); true },
            Key::End => { self.cursor_to_end(); true },
            Key::Enter => {
                let text = self.get_text().to_owned();
                if !text.is_empty() {
                    self.cmds.push_back(text);
                    self.push_hist();
                    self.move_cursor(|_| 0);
                }
                true
            },
            Key::Up => {
                if self.hist_pos+1 < self.hist.len() {
                    self.hist_pos += 1;
                }
                self.cursor_to_end();
                true
            },
            Key::Down => {
                if self.hist_pos > 0 {
                    self.hist_pos -= 1;
                }
                self.cursor_to_end();
                true
            },
            _ => false,
        }
    }


    pub fn get_text(&self) -> &str {
        &self.hist[self.hist_pos]
    }


    fn cursor_col(&self) -> isize {
        self.cursor_pos as isize
    }

    fn move_cursor<F>(&mut self, f: F) where F : Fn(isize) -> isize {
        let new = f(self.cursor_pos);
        if new > self.get_text().len() as isize {
            self.cursor_pos = self.get_text().len() as isize
        } else if new < 0 {
            self.cursor_pos = 0;
        } else {
            self.cursor_pos = new;
        }
    }

    fn cursor_to_end(&mut self) {
        self.cursor_pos = self.get_text().len() as isize;
    }


    /// Pushes a new command history entry and resets hist_pos.
    ///
    /// This has the effect of pushing the current line up into the history and
    /// clearing the line. If hist_pos is greater than 0, pushes the current
    /// line to the front of the history.
    fn push_hist(&mut self) {
        if self.hist_pos > 0 {
            let ent = self.hist[self.hist_pos].clone();
            self.hist[0] = ent;
        }
        self.hist.push_front(String::new());
        self.hist_pos = 0;
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use rustbox::{ Event, Key };

    fn press_key(entry: &mut TextEntry, key: Key) {
        entry.handle(&Event::KeyEvent(key));
    }

    fn press_times(entry: &mut TextEntry, key: Key, times: usize) {
        for _ in 0..times {
            entry.handle(&Event::KeyEvent(key));
        }
    }

    fn press_chars(entry: &mut TextEntry, string: &str) {
        for ch in string.chars() {
            entry.handle(&Event::KeyEvent(Key::Char(ch)));
        }
    }

    #[test]
    fn basic_typing() {
        let mut entry = TextEntry::new();
        press_chars(&mut entry, "testing");
        press_key(&mut entry, Key::Enter);
        assert_eq!(Some("testing".to_owned()), entry.next_entry());
    }

    #[test]
    fn insert_typing() {
        // Tests typing in the middle of the line.
        let mut entry = TextEntry::new();
        press_chars(&mut entry, "This is test");
        assert_eq!("This is test", entry.get_text());
        press_times(&mut entry, Key::Left, 4);
        press_chars(&mut entry, "a ");
        press_key(&mut entry, Key::End);
        assert_eq!("This is a test", entry.get_text());
        press_key(&mut entry, Key::Enter);
        assert_eq!(Some("This is a test".to_owned()), entry.next_entry());
    }

    #[test]
    fn move_past_ends() {
        let mut entry = TextEntry::new();
        press_chars(&mut entry, "test");
        press_times(&mut entry, Key::Left, 40);
        press_key(&mut entry, Key::Enter);
        assert_eq!(Some("test".to_owned()), entry.next_entry());

        press_chars(&mut entry, "test");
        press_times(&mut entry, Key::Right, 40);
        press_key(&mut entry, Key::Enter);
        assert_eq!(Some("test".to_owned()), entry.next_entry());
    }

    #[test]
    fn cycle_history() {
        let mut entry = TextEntry::new();
        press_chars(&mut entry, "Entry 1");
        press_key(&mut entry, Key::Enter);
        assert_eq!(Some("Entry 1".to_owned()), entry.next_entry());
        press_chars(&mut entry, "Another entry 2");
        press_key(&mut entry, Key::Enter);
        assert_eq!(Some("Another entry 2".to_owned()), entry.next_entry());
        press_chars(&mut entry, "Entry 3");
        press_key(&mut entry, Key::Enter);
        assert_eq!(Some("Entry 3".to_owned()), entry.next_entry());

        assert_eq!("", entry.get_text());
        press_key(&mut entry, Key::Up);
        assert_eq!("Entry 3", entry.get_text());
        press_key(&mut entry, Key::Up);
        assert_eq!("Another entry 2", entry.get_text());
        press_key(&mut entry, Key::Up);
        assert_eq!("Entry 1", entry.get_text());
        press_key(&mut entry, Key::Up);
        assert_eq!("Entry 1", entry.get_text());
        press_key(&mut entry, Key::Down);
        assert_eq!("Another entry 2", entry.get_text());
        press_key(&mut entry, Key::Down);
        assert_eq!("Entry 3", entry.get_text());
        press_key(&mut entry, Key::Down);
        assert_eq!("", entry.get_text());
    }

    #[test]
    fn enter_from_history() {
        // This checks that when we enter a line from our entry history, it gets
        // placed in the history properly.
        let mut entry = TextEntry::new();
        press_chars(&mut entry, "Entry 1");
        press_key(&mut entry, Key::Enter);
        assert_eq!(Some("Entry 1".to_owned()), entry.next_entry());
        press_chars(&mut entry, "Another entry 2");
        press_key(&mut entry, Key::Enter);
        assert_eq!(Some("Another entry 2".to_owned()), entry.next_entry());
        press_chars(&mut entry, "Entry 3");
        press_key(&mut entry, Key::Enter);
        assert_eq!(Some("Entry 3".to_owned()), entry.next_entry());

        // Cycle up to "Another entry 2"
        press_times(&mut entry, Key::Up, 2);
        assert_eq!("Another entry 2", entry.get_text());
        press_key(&mut entry, Key::Backspace);
        press_chars(&mut entry, "4");
        assert_eq!("Another entry 4", entry.get_text());
        press_key(&mut entry, Key::Enter);

        assert_eq!("", entry.get_text());
        press_key(&mut entry, Key::Up);
        assert_eq!("Another entry 4", entry.get_text());
    }
}
