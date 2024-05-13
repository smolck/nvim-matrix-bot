#![feature(try_blocks)]
#![allow(clippy::result_large_err)]

mod command;
mod config;
mod gif;
mod help;

use std::path::Path;

use serde_json::Value as Json;

const DEFAULT_HOMESERVER: &str = "https://matrix.org";

#[derive(serde::Deserialize)]
struct MxcUriCreateResponse {
    content_uri: String,
    #[allow(unused)]
    unused_expires_at: i64,
}

struct MatrixClient {
    access_token: Option<String>,
    command_parser: command::CommandParser,
    homeserver: String,
    agent: ureq::Agent,
    /// If this is None, then gifs won't be supported
    tenor_api_key: Option<String>,
    config: config::Config,
}

impl MatrixClient {
    fn new(homeserver: String, tenor_api_key: Option<String>, config: config::Config) -> Self {
        Self {
            access_token: None,
            // set timeouts?
            agent: ureq::AgentBuilder::new().build(),
            homeserver,
            command_parser: command::CommandParser::new(),
            tenor_api_key,
            config,
        }
    }

    fn login(&mut self, user: &str, password: &str) -> Result<(), ureq::Error> {
        let response: String = self
            .agent
            .post(&format!("{}/_matrix/client/r0/login", self.homeserver))
            // TODO(smolck): These headers necessary?
            .set("Accept", "application/json")
            .set("Content-Type", "application/json")
            .set("Charset", "utf-8")
            .send_string(
                &serde_json::json!({
                    "type": "m.login.password",
                    "user": user,
                    "password": password,
                })
                .to_string(),
            )?
            .into_string()?;

        let json = serde_json::from_str::<Json>(&response).unwrap();
        self.access_token = Some(json["access_token"].as_str().unwrap().to_string());

        Ok(())
    }

    fn send_gif_if_key_else_do_nothing(
        &self,
        search_query: &str,
        room_id: &str,
    ) -> Result<(), ureq::Error> {
        let Some(key) = &self.tenor_api_key else {
            return Ok(());
        };

        let gif = gif::Gif::search(&self.agent, key, search_query)?;
        let gif_bytes_reader = self.agent.get(&gif.url).call()?.into_reader();

        let mxc_uri = self
            .agent
            .post(&format!("{}/_matrix/media/v1/create", self.homeserver,))
            .set("Accept", "application/json")
            .set("Charset", "utf-8")
            .query("access_token", self.access_token.as_ref().unwrap())
            .call()
            .unwrap();
        let mxc_uri = serde_json::from_str::<MxcUriCreateResponse>(&mxc_uri.into_string().unwrap())
            .unwrap()
            .content_uri;

        // TODO(smolck): Umm . . . absolutely not lmao
        let mxc_uri_parts: Vec<&str> = mxc_uri.split("mxc://").collect();
        let parts = mxc_uri_parts[1].split('/').collect::<Vec<&str>>();
        let server_name = parts[0];
        let media_id = parts[1];

        // Upload gif to mxc uri
        self.agent
            .put(&format!(
                "{}/_matrix/media/v3/upload/{}/{}",
                self.homeserver, server_name, media_id,
            ))
            .query("filename", "nvim-bot-gif.gif")
            .query("access_token", self.access_token.as_ref().unwrap())
            .set("Content-Type", "application/octet-stream")
            .send(gif_bytes_reader)?;

        let json = serde_json::json!({
            "msgtype": "m.image",
            "info": {
                "mimetype": "image/gif",
                "size": gif.size,
                "h": gif.height,
                "w": gif.width,
            },
            "url": mxc_uri,
            "body": "nvim-bot-gif.gif",
        })
        .to_string();

        let _response = self
            .agent
            .post(&format!(
                "{}/_matrix/client/r0/rooms/{}/send/m.room.message",
                self.homeserver, room_id
            ))
            .set("Accept", "application/json")
            .set("Content-Type", "application/json")
            .set("Charset", "utf-8")
            .query("access_token", self.access_token.as_ref().unwrap())
            .send_string(&json)?
            .into_string()?;

        Ok(())
    }

    fn send_message(
        &self,
        use_markdown: bool,
        message: &str,
        room_id: &str,
    ) -> Result<(), ureq::Error> {
        let json = if use_markdown {
            let mut html_message = String::new();
            pulldown_cmark::html::push_html(
                &mut html_message,
                pulldown_cmark::Parser::new(message),
            );

            serde_json::json!({
                "msgtype": "m.text",
                "body": message,
                "format": "org.matrix.custom.html",
                "formatted_body": html_message,
            })
            .to_string()
        } else {
            serde_json::json!({
                "msgtype": "m.text",
                "body": message,
            })
            .to_string()
        };

        // TODO(smolck): Maybe deal with response or use it or something?
        let _response: String = self
            .agent
            .post(&format!(
                "{}/_matrix/client/r0/rooms/{}/send/m.room.message",
                self.homeserver, room_id
            ))
            .set("Accept", "application/json")
            .set("Content-Type", "application/json")
            .set("Charset", "utf-8")
            .query("access_token", self.access_token.as_ref().unwrap())
            .send_string(&json)?
            .into_string()?;

        Ok(())
    }

    fn sync_once(
        &self,
        next_batch: Option<&str>,
        filter: Option<&str>,
    ) -> Result<String, ureq::Error> {
        let mut req = self
            .agent
            .get(&format!("{}/_matrix/client/r0/sync", self.homeserver))
            .set("Accept", "application/json")
            .set("Content-Type", "application/json")
            .set("Charset", "utf-8")
            .query("access_token", self.access_token.as_ref().unwrap())
            .query("timeout", "10000");

        if let Some(filter) = filter {
            req = req.query("filter", filter);
        }

        if let Some(next_batch) = next_batch {
            req = req.query("since", next_batch);
        }

        let response: String = req.call()?.into_string()?;
        let response_json = serde_json::from_str::<Json>(&response).unwrap();

        self.handle_sync_response(&response_json);

        Ok(response_json["next_batch"].as_str().unwrap().to_string())
    }

    fn handle_cmd(&self, cmd: command::Command, room_id: &str) {
        use command::Command::*;
        match cmd {
            Help { ref docs } => {
                let mut tags = vec![];
                let mut not_found = vec![];
                for doc in docs {
                    if let Some(tag) = help::help(doc) {
                        tags.push(tag);
                    } else {
                        not_found.push(doc);
                    }
                }

                let body = tags
                    .into_iter()
                    .map(|tag| {
                        format!(
                            "* [`{help}`]({link}) in *{file}*",
                            help = tag.name,
                            link = tag.to_url(),
                            file = tag.file
                        )
                    })
                    .collect::<Vec<String>>()
                    .join("\n");

                if !body.is_empty() {
                    self.send_message(true, &body, room_id).unwrap_or_else(|err| {
                        eprintln!("Error sending message for {cmd:?} cmd: {err}");
                    });
                }

                if !not_found.is_empty() {
                    let not_found_body = format!(
                        "No help found for:\n{}",
                        not_found
                            .into_iter()
                            .map(|name| format!("* `{}`", name))
                            .collect::<Vec<String>>()
                            .join("\n")
                    );
                    self.send_message(true, &not_found_body, room_id).unwrap_or_else(|err| {
                        eprintln!("Error sending message for {cmd:?} cmd: {err}");
                    });
                }
            }
            Sandwich { to } => {
                if let Some(Err(err)) = self.config.rooms.get(room_id).and_then(|config| {
                    if config.sandwich {
                        Some(self.send_message(
                            true,
                            &format!("here's a sandwich, {}: 🥪", to),
                            room_id,
                        ))
                    } else {
                        None
                    }
                }) {
                    eprintln!("Error sending sandwich! {}", err);
                }
            }
            Url { url } => {
                self.send_message(true, url, room_id).unwrap_or_else(|err| {
                    eprintln!("Error sending URL {url}: {err}");
                });
            }
            Gif { search } => {
                if let Some(Err(err)) = self.config.rooms.get(room_id).and_then(|config| {
                    if config.gifs {
                        Some(self.send_gif_if_key_else_do_nothing(&search, room_id))
                    } else {
                        None
                    }
                }) {
                    eprintln!("Error sending gif! {}", err);
                }
            }
        }
    }

    fn handle_sync_response(&self, response: &Json) {
        if let Some(joined) = response["rooms"]
            .as_object()
            .and_then(|rooms| rooms["join"].as_object())
        {
            for (_room, room_id) in joined.values().zip(joined.keys()) {
                if let Some(events) = joined
                    .get(room_id)
                    .and_then(|room| {
                        room.get("timeline")
                            .and_then(|timeline| timeline.as_object())
                    })
                    .and_then(|timeline| timeline.get("events").and_then(|x| x.as_array()))
                {
                    for event in events {
                        let _: Option<_> = try {
                            let event_type = event.get("type")?.as_str().unwrap();
                            let _sender = event.get("sender")?.as_str().unwrap();
                            let content = event.get("content")?.as_object().unwrap();

                            // Use formatted_body if available
                            let (mut body, escape_reply) =
                                if let Some(body) = content.get("formatted_body") {
                                    (body.as_str().unwrap(), true)
                                } else {
                                    (content.get("body")?.as_str().unwrap(), false)
                                };

                            // Don't search in the message the user is replying to so we don't
                            // duplicate messages if the message being replied to had a
                            // help doc reference
                            if escape_reply {
                                // TODO(smolck): This feels like it could be broken pretty
                                // easily. But hopefully not? Since stuff like this *should* get
                                // escaped if it was typed by the user . . . I think. Maybe.
                                body = body
                                    .split("</mx_reply>")
                                    .collect::<Vec<_>>()
                                    .get(1)
                                    .unwrap_or(&body);
                            }

                            if event_type == "m.room.message" {
                                if let Some(cmd) = self.command_parser.parse(body) {
                                    self.handle_cmd(cmd, room_id);
                                }
                            }
                        };
                    }
                };
            }
        }
    }

    fn sync(&self) -> Result<(), ureq::Error> {
        let mut next_batch: Option<String> = None;
        loop {
            match self.sync_once(next_batch.as_deref(), None) {
                Ok(new_batch) => {
                    next_batch = Some(new_batch);
                }
                // TODO(smolck): I don't think we need to crash on this
                Err(err) => eprintln!("Error syncing! {}", err),
            }
            std::thread::sleep(std::time::Duration::from_millis(1000));
        }
    }
}

fn main() -> Result<(), ureq::Error> {
    let config = {
        if Path::exists(Path::new("./config.json")) {
            // read config from file
            config::Config::from_file("./config.json").unwrap()
        } else {
            println!("no config file, using defaults");
            config::Config::default()
        }
    };

    let user = std::env::var("MATRIX_USERNAME")
        .expect("Please set the environment variable MATRIX_USERNAME");

    let password = std::env::var("MATRIX_PASSWORD")
        .expect("Please set the environment variable MATRIX_PASSWORD");

    let homeserver = {
        match std::env::var("MATRIX_HOMESERVER") {
            Err(_) => {
                println!("defaulting to https://matrix.org for homeserver");
                String::from(DEFAULT_HOMESERVER)
            }
            Ok(homeserver) => homeserver,
        }
    };

    let tenor_api_key = {
        match std::env::var("TENOR_API_KEY") {
            Err(_) => {
                println!("running without tenor gif functionality");
                None
            }
            Ok(key) => Some(key),
        }
    };

    let mut client = MatrixClient::new(homeserver, tenor_api_key, config);
    client.login(&user, &password)?;
    client.sync()?;

    Ok(())
}
