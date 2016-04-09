//! This module exposes an interface for building status bars which can be
//! inserted in the UI either above or below the buffer view.
//!
//! Status bars must `impl` the `StatusBar` trait and define an `update`
//! function that updates their internal state and a `render` function that
//! displays them on the terminal at the given y index.

use super::TermUi;

pub trait StatusBar {
    /// Updates the status bar's state.
    fn update(&mut self, ui: &mut TermUi);

    /// Renders the bar at the given position.
    ///
    /// Nothing enforces that the bar actually renders at the given position.
    /// Bars can render anything wherever they like. Happy hacking!
    fn render(&mut self, y: usize, rb: &mut TermUi);
}


pub struct MainBar;

impl StatusBar for MainBar {
    fn update(&mut self, _: &mut TermUi) {
    }

    fn render(&mut self, y: usize, ui: &mut TermUi) {
        use rustbox::{ Color, RB_NORMAL };

        let w = ui.rb.width();

        // 'w' many blank spaces
        let blank = String::from_utf8(vec![b' '; w]).unwrap();
        ui.rb.print(0, y, RB_NORMAL, Color::Default, Color::Black, &blank);

        // let mut x = 0;

        let buf_name = ui.view.get_name();
        let text = format!(" {} ", buf_name);
        ui.rb.print(0, y, RB_NORMAL, Color::White, Color::Black, &text);
        // x += text.len();
    }
}
