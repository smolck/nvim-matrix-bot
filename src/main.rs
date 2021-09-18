use futures::lock::Mutex;
use std::env;
use std::sync::Arc;

use fancy_regex::Regex;
use nvim_rs::{
    compat::tokio::Compat, create::tokio as create, rpc::handler::Dummy as DummyHandler,
};
use serde_json::json;
use tokio::process::{ChildStdin, Command};

use matrix_sdk::{
    config::SyncSettings,
    room::Room,
    ruma::{
        /*RoomId,*/
        events::{
            room::message::{
                FormattedBody, MessageEventContent, MessageType, Relation, TextMessageEventContent,
            },
            SyncMessageEvent,
        },
        /*room_id,*/ UserId,
    },
    Client, Result,
};
use std::convert::TryFrom;

#[derive(Debug)]
struct Link {
    link: String,
    help_file: String,
    help: String,
}

async fn potentially_invalid_doc_links(
    nvim: &nvim_rs::Neovim<Compat<ChildStdin>>,
    regex: &Regex,
    string: &String,
) -> Option<Vec<Link>> {
    if regex.is_match(&string).unwrap() {
        // TODO(smolck): Yeah I know naming and all that
        let somethings = regex
            .captures_iter(&string)
            .map(|caps| {
                let caps = caps.as_ref().unwrap();
                (caps.get(0).unwrap().as_str(), caps.get(1).unwrap().as_str())
            })
            .collect::<Vec<(&str, &str)>>();

        let mut links = Vec::with_capacity(somethings.len());
        for (full_help_with_backticks, doc_name) in somethings {
            // TODO(smolck): ew format! and .to_string() allocations or something
            if let Ok(()) = nvim
                .command(&full_help_with_backticks.replace("`", "").replace(":", ""))
                .await
            {
                let fname_root = nvim.command_output("echo expand(\"%:t:r\")").await.unwrap();
                links.push(Link {
                    link: format!(
                        "https://neovim.io/doc/user/{}.html#{}",
                        fname_root, doc_name
                    ),
                    help_file: fname_root.to_owned() + ".txt",
                    help: doc_name.to_owned(),
                });
            }
        }

        Some(links)
    } else {
        None
    }
}

// see https://github.com/matrix-org/matrix-rust-sdk/blob/70ab0f446dc83ddf5d522bc783d0ab4bef1ff6b2/crates/matrix-sdk/examples/image_bot.rs#L23
async fn on_room_message(
    event: SyncMessageEvent<MessageEventContent>,
    room: Room,
    nvim: Arc<Mutex<nvim_rs::Neovim<Compat<ChildStdin>>>>,
) {
    if let Some(Relation::Replacement(_)) = event.content.relates_to {
        // Don't do this for edits.
        return;
    }

    if let Room::Joined(room) = room {
        // see https://github.com/matrix-org/matrix-rust-sdk/blob/15e9d03c2cc4000b04889af6d0c32fa4a96632ce/crates/matrix-sdk/examples/command_bot.rs#L16
        if let SyncMessageEvent {
            content:
                MessageEventContent {
                    msgtype: MessageType::Text(TextMessageEventContent { body: msg_body, .. }),
                    ..
                },
            ..
        } = event
        {
            // TODO(smolck): Perf of creating this regex every time? Does it matter?
            let regex = Regex::new(r"(?:`:(?:help|h|he|hel) (((?!`).)*)`)").unwrap();

            let nvim = nvim.lock().await;

            let links = potentially_invalid_doc_links(&nvim, &regex, &msg_body).await;
            if let Some(links) = links {
                if links.len() == 0 {
                    println!("No help docs in '{}'", msg_body);
                    return;
                }

                let reply_body = links
                    .iter()
                    .map(|l| {
                        format!(
                            "* [`{help}`]({link}) in *{file}*",
                            help = l.help,
                            link = l.link,
                            file = l.help_file
                        )
                    })
                    .collect::<Vec<String>>()
                    .join("\n");
                let reply_body = format!("Links to referenced help pages:\n{}", reply_body);
                let formatted_reply_body = FormattedBody::markdown(&reply_body).unwrap();

                room.send_raw(
                    // See https://matrix.org/docs/spec/client_server/r0.6.1#rich-replies
                    json!({
                        "msgtype": "m.text",
                        "body": reply_body,
                        "formatted_body": formatted_reply_body.body,
                        "format": "org.matrix.custom.html",
                        /*"m.relates_to": {
                            "m.in_reply_to": {
                                "event_id": event.event_id
                            }
                        }*/
                    }),
                    "m.room.message",
                    None,
                )
                .await
                .unwrap();
            }
        }
    }
    /*if room.room_id() == &room_id!("!neovim-chat:matrix.org") {
        println!("Received a message from neovim-chat: {:?}", ev);
    }*/
}

#[tokio::main]
async fn main() -> Result<()> {
    let (nvim, _io_handle, _child) = create::new_child_cmd(
        Command::new("nvim")
            .args(&["-u", "NONE", "--embed", "--headless"])
            .env("NVIM_LOG_FILE", "nvimlog"),
        DummyHandler::new(),
    )
    .await
    .unwrap();

    // let regex = Regex::new(r"(?:`:help (((?!`).)*)`)").unwrap();
    let nvim = Arc::new(Mutex::new(nvim));
    let username = env::var("MATRIX_USERNAME").expect("Need MATRIX_USERNAME set");
    let password = env::var("MATRIX_PASSWORD").expect("Need MATRIX_PASSWORD set");

    let user_id = UserId::try_from(format!("@{}:matrix.org", username))?; // Assumes matrix.org homeserver
    let client = Client::new_from_user_id(&user_id).await?;
    client
        .login(user_id.localpart(), &password, None, None)
        .await?;

    client
        .register_event_handler(
            move |ev: SyncMessageEvent<MessageEventContent>, room: Room| {
                on_room_message(ev, room, nvim.clone())
            },
        )
        .await;

    client.sync(SyncSettings::default()).await;

    Ok(())
}
