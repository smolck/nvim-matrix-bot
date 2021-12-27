#![feature(try_blocks)]

mod command;
mod tag_search;

use serde_json::Value as Json;

const HOMESERVER: &str = "https://matrix.org";

struct MatrixClient<'a> {
    pub access_token: Option<String>,
    pub command_parser: command::CommandParser,
    pub tags: Vec<tag_search::Tag<'a>>,
}

impl<'a> MatrixClient<'a> {
    fn new(tags: Vec<tag_search::Tag<'a>>) -> Self {
        Self {
            tags,
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
    /*
    let body = body.as_ref();
    let mut html_body = String::new();

    pulldown_cmark::html::push_html(&mut html_body, pulldown_cmark::Parser::new(body));

    (html_body != format!("<p>{}</p>\n", body)).then(|| Self::html(html_body))
    */

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
                let mut indexes = vec![];
                let mut not_found = vec![];
                for doc in docs {
                    let needle = tag_search::Tag::from_name(doc);
                    if let Ok(idx) = self.tags.binary_search(&needle) {
                        indexes.push(idx);
                    } else {
                        not_found.push(doc);
                    }
                }

                let body = indexes
                    .into_iter()
                    .map(|idx| {
                        let tag = &self.tags[idx];

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

                if not_found.len() > 0 {
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
                self.send_message(true, &url, room_id).unwrap();
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
                            let body = content.get("body")?.as_str().unwrap();

                            if event_type == "m.room.message" {
                                if let Some(cmd) = self.command_parser.parse(&body) {
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
    let tags = std::fs::read_to_string("/usr/local/share/nvim/runtime/doc/tags").unwrap();
    let tags = tags
        .split("\n")
        .filter(|line| !line.is_empty())
        .map(|line| tag_search::Tag::from_str(line))
        .collect::<Vec<tag_search::Tag>>();

    let user =
        std::env::var("MATRIX_USER").expect("Please set the environment variable MATRIX_USER");

    let password = std::env::var("MATRIX_PASSWORD")
        .expect("Please set the environment variable MATRIX_PASSWORD");

    let mut client = MatrixClient::new(tags);
    client.login(&user, &password)?;
    // client.sync_once(None, Some("{\"room\":{\"timeline\":{\"limit\":1}}}"))?;
    // client.sync_once(None, None)?;
    client.sync()?;

    Ok(())
}
