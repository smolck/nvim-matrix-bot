#![feature(try_blocks)]
#![allow(clippy::result_large_err)]

mod command;
mod help;

use serde_json::Value as Json;

const HOMESERVER: &str = "https://matrix.org";

struct MatrixClient {
    pub access_token: Option<String>,
    pub command_parser: command::CommandParser,
}

impl MatrixClient {
    fn new() -> Self {
        Self {
            access_token: None,
            command_parser: command::CommandParser::new(),
        }
    }

    fn login(&mut self, user: &str, password: &str) -> Result<(), ureq::Error> {
        let response: String = ureq::post(&format!("{}/_matrix/client/r0/login", HOMESERVER))
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
        let _response: String = ureq::post(&format!(
            "{}/_matrix/client/r0/rooms/{}/send/m.room.message",
            HOMESERVER, room_id
        ))
        .set("Accept", "application/json")
        .set("Content-Type", "application/json")
        .set("Charset", "utf-8")
        .query("access_token", self.access_token.as_ref().unwrap())
        // .set("Authorization", &format!("Bearer {}", self.access_token.as_ref().unwrap()))
        .send_string(&json)?
        .into_string()?;

        // let json = serde_json::from_str::<Json>(&response).unwrap();

        Ok(())
    }

    fn sync_once(
        &self,
        next_batch: Option<&str>,
        filter: Option<&str>,
    ) -> Result<String, ureq::Error> {
        let mut req = ureq::get(&format!("{}/_matrix/client/r0/sync", HOMESERVER))
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
            Help { docs } => {
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
                    self.send_message(true, &body, room_id).unwrap();
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
                    self.send_message(true, &not_found_body, room_id).unwrap();
                }
            }
            Sandwich { to } => {
                self.send_message(true, &format!("here's a sandwich, {}: 🥪", to), room_id)
                    .unwrap();
            }
            Url { url } => {
                self.send_message(true, url, room_id).unwrap();
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
                            let (mut body, escape_reply) = if let Some(body) = content.get("formatted_body") {
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
                                body = body.split("</mx_reply>").collect::<Vec<_>>().get(1).unwrap_or(&body);
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
            next_batch = Some(self.sync_once(next_batch.as_deref(), None)?);
            std::thread::sleep(std::time::Duration::from_millis(1000));
        }
    }
}

fn main() -> Result<(), ureq::Error> {
    let user = std::env::var("MATRIX_USERNAME")
        .expect("Please set the environment variable MATRIX_USERNAME");

    let password = std::env::var("MATRIX_PASSWORD")
        .expect("Please set the environment variable MATRIX_PASSWORD");

    let mut client = MatrixClient::new();
    client.login(&user, &password)?;
    client.sync()?;

    Ok(())
}
