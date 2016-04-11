extern crate rustbox;
extern crate time;

pub mod ui;
pub mod model;

use self::ui::TermUi;

fn main() {
    let mut ui = TermUi::new().expect("Failed to initialize UI");
    ui.main();
}
