//! This module exposes an interface for building status bars which can be
//! inserted in the UI either above or below the buffer view.
//!
//! Status bars must `impl` the `StatusBar` trait and define an `update`
//! function that updates their internal state and a `render` function that
//! displays them on the terminal at the given y index.

use super::TermUi;
use super::util::RustBoxExt;

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
        use rustbox::{RB_NORMAL};
        use rustbox::Color::*;
        // use super::util::AlignCol::*;

        let buf = ui.view.buf.borrow();
        let buf_name = buf.name();

        // TODO: Right align scroll display
        let buf_scroll = match ui.view.scroll.clone() {
            Some(line) => format!("{}", ui.view.scroll_height()),
            None => "BOT".to_owned(),
        };

        ui.rb.blank_line(y, RB_NORMAL, Default, Black);
        ui.rb.print_cols(y)
            .skip(1)
            .print_col(RB_NORMAL, White, Black, buf_name)
            .skip(1)
            .print_col(RB_NORMAL, White, Black, " | ")
            .skip(1)
            .print_col(RB_NORMAL, White, Black, &buf_scroll)
            ;
    }
}
