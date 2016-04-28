use std::fs::File;
use std::io::Read;
use xdg::BaseDirectories;
use toml;
use toml::Parser;
use rustc_serialize::Decodable;

pub type UserId = String;


/// Loads the client's config file. Panics on error.
pub fn read_config() -> Config {
    let dirs = BaseDirectories::with_prefix("distirc-client").unwrap();
    let path = dirs.find_config_file("config.toml")
        .expect("Missing configuration file");

    info!("Reading config file from {}", path.display());
    let mut s = String::new();

    let mut f = File::open(path).expect("Failed to open config file");
    f.read_to_string(&mut s).expect("Failed to read config file");
    debug!("Read config");

    let mut parser = Parser::new(&s);
    if let Some(table) = parser.parse() {
        debug!("Parsed config");

        let mut dec = toml::Decoder::new(toml::Value::Table(table));
        Config::decode(&mut dec).expect("Invalid config file")
    } else {
        error!("Failed to parse config file. Error list:");
        for e in parser.errors {
            error!("{}", e);
        }
        panic!("Failed to parse configuration file.");
    }
}


#[derive(Debug, Clone, RustcEncodable, RustcDecodable)]
pub struct Config {
    pub core: CoreConfig,
}

#[derive(Debug, Clone, RustcEncodable, RustcDecodable)]
pub struct CoreConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub pass: String,
}
