use fancy_regex::Regex;

#[derive(Debug, PartialEq, Eq)]
pub enum Command<'a> {
    Help { docs: Vec<&'a str> },
    Sandwich { to: &'a str },
    Url { url: &'a str },
}

pub struct CommandParser {
    command_regex: Regex,
    url_commands_json: serde_json::Value,
}

impl CommandParser {
    pub fn new() -> Self {
        let commands_file = include_str!("../commands.json");
        let json = serde_json::from_str(commands_file).unwrap();

        Self {
            url_commands_json: json,
            command_regex: Regex::new(r"^!(\w+)( *)(.*)").unwrap(),
        }
    }

    pub fn parse<'a>(&'a self, string: &'a str) -> Option<Command<'a>> {
        if self.command_regex.is_match(string).unwrap() {
            let mut iter = self.command_regex.captures_iter(string);
            let caps = iter.next()?.ok()?;
            let command = caps.get(1)?.as_str();
            let args: Option<Vec<&str>> = caps.get(3).map(|args| {
                args.as_str()
                    .split(" ")
                    .filter(|str| !str.is_empty())
                    .collect()
            });

            use Command::*;
            match command {
                "help" | "h" | "he" | "hel" => {
                    let docs = args?;
                    if docs.len() > 0 {
                        Some(Help { docs })
                    } else {
                        None
                    }
                }
                "sandwich" => {
                    let args = args?;
                    Some(Sandwich { to: args[0] })
                }
                x => {
                    if let Some(url) = self.url_commands_json.get(x) {
                        Some(Url { url: url.as_str().unwrap() })
                    } else {
                        None
                    }
                }
            }
        } else {
            None
        }
    }
}
