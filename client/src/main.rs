extern crate rustbox;
extern crate time;

mod ui;

use self::ui::TermUi;

fn main() {
    let mut ui = TermUi::new().expect("Failed to initialize UI");
    ui.main();
}
