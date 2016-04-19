extern crate syntex;
extern crate serde_codegen;

use std::env;
use std::path::Path;

pub fn main() {
    let out_dir = env::var_os("OUT_DIR").unwrap();

    let files = vec![
        (Path::new("src/line.rs.in"), Path::new(&out_dir).join("line.rs")),
        (Path::new("src/messages.rs.in"), Path::new(&out_dir).join("messages.rs")),
    ];

    for (src, dst) in files {
        let mut registry = syntex::Registry::new();
        serde_codegen::register(&mut registry);
        registry.expand("", &src, &dst).unwrap();
    }
}
