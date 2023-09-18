use fancy_regex::Regex;

#[derive(Debug, PartialEq, Eq)]
pub enum Command<'a> {
    Help { docs: Vec<&'a str> },
    Sandwich { to: &'a str },
    Url { url: &'a str },
}

pub struct CommandParser {
    command_regex: Regex,
    backticked_help_regex: Regex,
    codeblock_help_regex: Regex, // For when we're parsing a formatted_body, and instead of
    // backticks we get something like <code>:help blah</code>
    url_commands_json: serde_json::Value,
}

impl CommandParser {
    pub fn new() -> Self {
        let commands_file = include_str!("../commands.json");
        let json = serde_json::from_str(commands_file).unwrap();

        Self {
            url_commands_json: json,
            command_regex: Regex::new(r"^!(\w+)( *)(.*)").unwrap(),
            backticked_help_regex: Regex::new(r"(?:`:(?:help|h|he|hel) (((?!`).)*)`)").unwrap(),
            codeblock_help_regex: Regex::new(
                r"(?:<code>:(?:help|h|he|hel) (((?!(<\/code>)).)*)<\/code>)",
            )
            .unwrap(),
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
                    let mut docs = args?;

                    // Get rid of duplicates (see https://stackoverflow.com/a/47636725)
                    docs.sort_unstable();
                    docs.dedup();

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
                        Some(Url {
                            url: url.as_str().unwrap(),
                        })
                    } else {
                        None
                    }
                }
            }
        } else if self.backticked_help_regex.is_match(&string).unwrap() {
            let mut docs = self
                .backticked_help_regex
                .captures_iter(&string)
                .map(|caps| {
                    let caps = caps.as_ref().unwrap();
                    caps.get(1).unwrap().as_str()
                })
                .collect::<Vec<&str>>();

            // Get rid of duplicates (see https://stackoverflow.com/a/47636725)
            docs.sort_unstable();
            docs.dedup();

            Some(Command::Help { docs })
        } else if self.codeblock_help_regex.is_match(&string).unwrap() {
            let mut docs = self
                .codeblock_help_regex
                .captures_iter(&string)
                .map(|caps| {
                    let caps = caps.as_ref().unwrap();
                    caps.get(1).unwrap().as_str()
                })
                .collect::<Vec<&str>>();

            // Get rid of duplicates (see https://stackoverflow.com/a/47636725)
            docs.sort_unstable();
            docs.dedup();

            Some(Command::Help { docs })
        } else {
            None
        }
    }
}
