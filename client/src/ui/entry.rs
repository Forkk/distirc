//! This module implements the text entry widget.

use std::collections::VecDeque;
use rustbox::{ RustBox, Event, Key, Style, Color };

use super::wrap::StringWrap;

/// The IRC client's text box.
pub struct TextEntry {
    // FIXME: Store cursor position as a pair of terminal column and string index.
    cursor_idx: usize,
    cursor_col: isize,
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
            cursor_idx: 0,
            cursor_col: 0,
            hist: hist,
            hist_pos: 0,
            cmds: VecDeque::new(),
        }
    }

    /// Returns the next entry in the list of things the user has typed.
    pub fn next_entry(&mut self) -> Option<String> {
        self.cmds.pop_front()
    }


    /// Queries the height of the entry in the given terminal.
    pub fn height(&self, rb: &mut RustBox) -> usize {
        StringWrap::new(self.get_text(), rb.width()).line_count()
    }

    /// Renders the text entry in the given terminal.
    pub fn render(&self, rb: &mut RustBox) {
        let w = rb.width();
        let h = rb.height();

        let wrap = StringWrap::new(self.get_text(), w);
        let ent_y = h - wrap.line_count();

        for (i, line) in wrap.iter_lines(self.get_text()).enumerate() {
            rb.print(0, ent_y + i - 1, Style::empty(), Color::Default, Color::Default, line);
        }

        let (x, y) = wrap.idx_pos(self.cursor_col() as usize);
        rb.set_cursor(x, ent_y as isize + y);
    }

    /// Handles an event
    ///
    /// Returns true if the event was handled.
    pub fn handle(&mut self, evt: &Event) -> bool {
        let ret = match *evt {
            Event::KeyEvent(key) => self.handle_key(&key),
            _ => false,
        };
        debug_assert!(self.cursor_col >= 0);
        debug_assert!(self.cursor_idx <= self.get_text().len());
        debug_assert!{
            // Check that cursor_col lines up with the number of the char at
            // cursor_idx.
            if let Some((idx, _)) = self.hist[self.hist_pos].char_indices().nth(self.cursor_col as usize) {
                idx == self.cursor_idx
            } else {
                self.cursor_idx == self.hist[self.hist_pos].len()
            },
            "Cursor column misaligned with cursor index"
        }
        ret
    }

    fn handle_key(&mut self, key: &Key) -> bool {
        match *key {
            Key::Char(ch) => {
                self.hist[self.hist_pos].insert(self.cursor_idx as usize, ch);
                self.move_cursor_by(1);
                true
            },
            Key::Backspace => {
                if self.cursor_col > 0 {
                    self.move_cursor_by(-1);
                    self.hist[self.hist_pos].remove(self.cursor_idx as usize);
                }
                true
            },
            Key::Left => { self.move_cursor_by(-1); true },
            Key::Right => { self.move_cursor_by(1); true },
            Key::Home => { self.move_cursor_home(); true },
            Key::End => { self.move_cursor_end(); true },
            Key::Enter => {
                let text = self.get_text().to_owned();
                if !text.is_empty() {
                    self.cmds.push_back(text);
                    self.push_hist();
                    self.move_cursor_home();
                }
                true
            },
            Key::Up => {
                if self.hist_pos+1 < self.hist.len() {
                    self.hist_pos += 1;
                }
                self.move_cursor_end();
                true
            },
            Key::Down => {
                if self.hist_pos > 0 {
                    self.hist_pos -= 1;
                }
                self.move_cursor_end();
                true
            },
            _ => false,
        }
    }


    pub fn get_text(&self) -> &str {
        &self.hist[self.hist_pos]
    }


    fn cursor_col(&self) -> isize {
        self.cursor_col
    }

    fn move_cursor_by(&mut self, by: isize) {
        let ref text = self.hist[self.hist_pos];
        if by > 0 {
            // Moving right, we slice the string from our current index to the
            // end and find the next boundary in the sub-string.
            let mut idxs = text[self.cursor_idx..].char_indices();
            if let Some((new, _)) = idxs.nth(by as usize) {
                // Add the previous cursor index to the index within the subslice.
                self.cursor_idx = self.cursor_idx + new;
                self.cursor_col += by;
            } else {
                self.cursor_idx = text.len();
                self.cursor_col = text.chars().count() as isize;
            }
        } else if by < 0 {
            let mut idxs = text[..self.cursor_idx].char_indices().rev();
            if let Some((new, _)) = idxs.nth((-by) as usize - 1) {
                self.cursor_idx = new;
                self.cursor_col += by;
            } else {
                self.cursor_idx = 0;
                self.cursor_col = 0;
            }
        }
    }

    fn move_cursor_home(&mut self) {
        self.cursor_idx = 0;
        self.cursor_col = 0;
    }

    fn move_cursor_end(&mut self) {
        self.cursor_idx = self.get_text().len();
        self.cursor_col = self.get_text().chars().count() as isize;
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
    fn backspace() {
        // Tests typing in the middle of the line.
        let mut entry = TextEntry::new();
        press_chars(&mut entry, "IRC sucks");
        press_times(&mut entry, Key::Backspace, 5);
        press_chars(&mut entry, "is awesome!");
        press_key(&mut entry, Key::Enter);
        assert_eq!(Some("IRC is awesome!".to_owned()), entry.next_entry());
    }

    #[test]
    fn multibyte_char_basic_typing() {
        let mut entry = TextEntry::new();
        press_chars(&mut entry, "こんにちわ");
        press_key(&mut entry, Key::Enter);
        assert_eq!(Some("こんにちわ".to_owned()), entry.next_entry());
    }

    #[test]
    fn multibyte_char_insert_typing() {
        let mut entry = TextEntry::new();
        press_chars(&mut entry, "これはです");
        press_times(&mut entry, Key::Left, 2);
        press_chars(&mut entry, "テスト");
        press_key(&mut entry, Key::Enter);
        assert_eq!(Some("これはテストです".to_owned()), entry.next_entry());
    }

    #[test]
    fn cursor_move_left() {
        let mut entry = TextEntry::new();

        press_chars(&mut entry, "This is test");
        press_times(&mut entry, Key::Left, 4);
        press_chars(&mut entry, "a ");
        press_key(&mut entry, Key::Enter);
        assert_eq!(Some("This is a test".to_owned()), entry.next_entry());

        press_chars(&mut entry, "これはです");
        press_times(&mut entry, Key::Left, 2);
        press_chars(&mut entry, "テスト");
        press_key(&mut entry, Key::Enter);
        assert_eq!(Some("これはテストです".to_owned()), entry.next_entry());
    }

    #[test]
    fn cursor_move_right() {
        // Also tests the home key.
        let mut entry = TextEntry::new();

        press_chars(&mut entry, "This is test");
        press_key(&mut entry, Key::Home);
        press_times(&mut entry, Key::Right, 8);
        press_chars(&mut entry, "a ");
        press_key(&mut entry, Key::Enter);
        assert_eq!(Some("This is a test".to_owned()), entry.next_entry());

        press_chars(&mut entry, "これはです");
        press_key(&mut entry, Key::Home);
        press_times(&mut entry, Key::Right, 3);
        press_chars(&mut entry, "テスト");
        println!("Text: {}", entry.get_text());
        press_key(&mut entry, Key::Enter);
        assert_eq!(Some("これはテストです".to_owned()), entry.next_entry());
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
        press_chars(&mut entry, "Another entry");
        press_key(&mut entry, Key::Enter);
        assert_eq!(Some("Another entry".to_owned()), entry.next_entry());
        press_chars(&mut entry, "Entry 3");
        press_key(&mut entry, Key::Enter);
        assert_eq!(Some("Entry 3".to_owned()), entry.next_entry());

        // Cycle up to "Another entry 2"
        press_times(&mut entry, Key::Up, 2);
        assert_eq!("Another entry", entry.get_text());
        press_chars(&mut entry, " 2");
        assert_eq!("Another entry 2", entry.get_text());
        press_key(&mut entry, Key::Enter);

        assert_eq!("", entry.get_text());
        press_key(&mut entry, Key::Up);
        assert_eq!("Another entry 2", entry.get_text());
    }
}
