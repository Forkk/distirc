//! This module exposes an interface for building status bars which can be
//! inserted in the UI either above or below the buffer view.
//!
//! Status bars must `impl` the `StatusBar` trait and define an `update`
//! function that updates their internal state and a `render` function that
//! displays them on the terminal at the given y index.

use std::cmp;
use rustbox::RustBox;

use super::TermUi;
use super::util::{RustBoxExt, LineBuilder};

pub trait StatusBar {
    /// Updates the status bar's state.
    fn update(&mut self, ui: &mut TermUi);

    /// Calculates the height of the bar in the given terminal.
    fn height(&self, ui: &TermUi) -> usize;

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

    fn height(&self, _ui: &TermUi) -> usize { 1 }

    fn render(&mut self, y: usize, ui: &mut TermUi) {
        use rustbox::{RB_NORMAL};
        use rustbox::Color::*;
        // use super::util::AlignCol::*;

        let buf = ui.view.buf.borrow();
        let buf_name = buf.name();

        // TODO: Right align scroll display
        let buf_scroll = match ui.view.scroll.clone() {
            Some(_) => format!("{}", ui.view.scroll_height()),
            None => "BOT".to_owned(),
        };

        ui.rb.blank_line(y, RB_NORMAL, Default, Black);

        let mut lb = LineBuilder::new();
        lb.skip(1);
        lb.add_column(buf_name.to_owned()).fgcolor(White).bgcolor(Black);
        lb.skip(1);
        lb.add_column(" | ".to_owned());
        lb.add_column(buf_scroll).fgcolor(White).bgcolor(Black);

        lb.print(y, &mut ui.rb);
    }
}


const ALERT_LIST_MAX_H: usize = 5;

pub struct AlertBar;

impl StatusBar for AlertBar {
    fn update(&mut self, _: &mut TermUi) {
    }

    fn height(&self, ui: &TermUi) -> usize {
        cmp::min(ui.alerts.count(), ALERT_LIST_MAX_H)
    }

    fn render(&mut self, y: usize, ui: &mut TermUi) {
        use rustbox::{RB_NORMAL};
        use rustbox::Color::*;
        // use super::util::AlignCol::*;

        for i in 0..self.height(ui) {
            ui.rb.blank_line(y+i, RB_NORMAL, White, Black);

            let alert = ui.alerts.get(i);
            let mut lb = LineBuilder::new();
            lb.add_column(format!("{}: ", i+1)).fgcolor(White).bgcolor(Black).pad_left(4);
            lb.add_column(alert.msg.clone()).fgcolor(White).bgcolor(Black);

            lb.print(y+i, &mut ui.rb);
        }


        // let mut lb = LineBuilder::new();
        // lb.skip(1);
        // lb.add_column(buf_name.to_owned()).fgcolor(White).bgcolor(Black);
        // lb.skip(1);
        // lb.add_column(" | ".to_owned());
        // lb.add_column(buf_scroll).fgcolor(White).bgcolor(Black);

        // lb.print(y, &mut ui.rb);
    }
}
