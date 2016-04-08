extern crate rustbox;

mod ui;

use self::ui::TermUi;

fn main() {
    let mut ui = TermUi::new().expect("Failed to initialize UI");
    ui.main();
}
