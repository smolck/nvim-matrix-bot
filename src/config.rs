use serde::Deserialize;
use std::collections::HashMap;

pub type RoomId = String;

#[derive(Deserialize)]
pub struct RoomConfig {
    pub gifs: bool,
    pub sandwich: bool,
}

#[derive(Default, Deserialize)]
pub struct Config {
    pub rooms: HashMap<RoomId, RoomConfig>,
}

impl Config {
    pub fn from_file(filepath: &str) -> Result<Self, serde_json::Error> {
        let contents = std::fs::read_to_string(filepath).unwrap();
        serde_json::from_str(&contents)
    }
}
