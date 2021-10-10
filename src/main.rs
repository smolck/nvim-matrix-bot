use futures::lock::Mutex;
use std::env;
use std::sync::Arc;

use fancy_regex::Regex;
use nvim_rs::{
    compat::tokio::Compat, create::tokio as create, rpc::handler::Dummy as DummyHandler,
};
use tokio::process::{ChildStdin, Command};

use matrix_sdk::{
    config::SyncSettings,
    room::{Joined as JoinedRoom, Room},
    ruma::{
        /*RoomId,*/
        events::{
            room::message::{
                FormattedBody, MessageEventContent, MessageType, Relation, TextMessageEventContent,
            },
            AnyMessageEventContent, SyncMessageEvent,
        },
        /*room_id,*/ UserId,
    },
    Client, Result,
};
use std::convert::TryFrom;

#[derive(Debug, PartialEq, Eq, Hash)]
struct Link {
    link: String,
    help_file: String,
    help: String,
}

#[derive(Debug, PartialEq, Eq)]
enum BotCommand<'a> {
    Help { docs: Vec<&'a str> },
    Sandwich { to: &'a str },
}

impl<'a> BotCommand<'a> {
    fn parse(regex: &Regex, string: &'a str) -> Option<BotCommand<'a>> {
        use BotCommand::*;

        if regex.is_match(string).unwrap() {
            let mut iter = regex.captures_iter(string);
            let caps = iter.next()?.ok()?;
            let command = caps.get(1)?.as_str();
            let args: Option<Vec<&str>> = caps.get(3).map(|args| {
                args.as_str()
                    .split(" ")
                    .filter(|str| !str.is_empty())
                    .collect()
            });

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
                _ => None,
            }
        } else {
            None
        }
    }
}

async fn get_links_for(
    nvim: &nvim_rs::Neovim<Compat<ChildStdin>>,
    docs_names: Vec<&str>,
) -> Vec<Link> {
    let mut links = Vec::with_capacity(docs_names.len());

    for doc_name in docs_names {
        // Don't push duplicates
        if let Some(_) = links.iter().find(|l: &&Link| l.help == doc_name) {
            continue;
        }

        let cmd = format!("help {}", doc_name);
        if let Ok(()) = nvim.command(&cmd).await {
            let fname_root = nvim.command_output("echo expand(\"%:t:r\")").await.unwrap();

            links.push(Link {
                link: format!(
                    "https://neovim.io/doc/user/{}.html#{}",
                    // We special-case "index" because index.html points to help.txt on the
                    // website because reasons.
                    if fname_root == "index" {
                        "vimindex"
                    } else {
                        fname_root.as_str()
                    },
                    doc_name
                ),
                help_file: fname_root.to_owned() + ".txt",
                help: doc_name.to_owned(),
            });
        }
    }

    links
}

async fn potentially_invalid_doc_links(
    nvim: &nvim_rs::Neovim<Compat<ChildStdin>>,
    regex: &Regex,
    string: &str,
) -> Option<Vec<Link>> {
    // TODO(smolck): Perf of creating this regex every time? Does it matter?

    if regex.is_match(&string).unwrap() {
        // TODO(smolck): Yeah I know naming and all that
        let somethings = regex
            .captures_iter(&string)
            .map(|caps| {
                let caps = caps.as_ref().unwrap();
                caps.get(1).unwrap().as_str()
            })
            .collect::<Vec<&str>>();

        let links = get_links_for(&nvim, somethings).await;
        Some(links)
    } else {
        None
    }
}

async fn maybe_send_help_reply(room: JoinedRoom, links: Vec<Link>) {
    if links.len() == 0 {
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

    room.send(
        AnyMessageEventContent::RoomMessage(MessageEventContent::text_html(
            reply_body,
            formatted_reply_body.body,
        )),
        None,
    )
    .await
    .unwrap();
}

// see https://github.com/matrix-org/matrix-rust-sdk/blob/70ab0f446dc83ddf5d522bc783d0ab4bef1ff6b2/crates/matrix-sdk/examples/image_bot.rs#L23
async fn on_room_message(
    event: SyncMessageEvent<MessageEventContent>,
    room: Room,
    state: Arc<Mutex<State>>,
) {
    // Don't send a message on edits.
    if let Some(Relation::Replacement(_)) = event.content.relates_to {
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
            let state = state.lock().await;

            let links =
                potentially_invalid_doc_links(&state.nvim, &state.backticked_help_regex, &msg_body)
                    .await;
            if let Some(links) = links {
                maybe_send_help_reply(room, links).await;
            } else {
                let maybe_command = BotCommand::parse(&state.bot_command_regex, &msg_body);

                if let Some(command) = maybe_command {
                    match command {
                        BotCommand::Help { docs } => {
                            let links = get_links_for(&state.nvim, docs).await;
                            maybe_send_help_reply(room, links).await;
                        }
                        BotCommand::Sandwich { to } => {
                            if event.sender
                                // TODO(smolck): probably have some sort of list of people that can
                                // call this; for now though, only I can trigger it
                                == UserId::try_from("@shadowwolf_01:matrix.org").unwrap()
                            {
                                room.send(
                                    AnyMessageEventContent::RoomMessage(
                                        MessageEventContent::text_plain(&format!(
                                            "here's a sandwich for you, {}: 🥪",
                                            to
                                        )),
                                    ),
                                    None,
                                )
                                .await
                                .unwrap();
                            }
                        }
                    }
                }
            }
        }
    }
}

struct State {
    pub nvim: nvim_rs::Neovim<Compat<ChildStdin>>,
    pub bot_command_regex: Regex,
    pub backticked_help_regex: Regex,
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

    let backticked_help_regex = Regex::new(r"(?:`:(?:help|h|he|hel) (((?!`).)*)`)").unwrap();
    let bot_command_regex = Regex::new(r"^!(\w+)( *)(.*)").unwrap();

    let state = Arc::new(Mutex::new(State {
        nvim,
        backticked_help_regex,
        bot_command_regex,
    }));

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
                on_room_message(ev, room, state.clone())
            },
        )
        .await;

    client.sync(SyncSettings::default()).await;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bot_command_parse_works() {
        assert_eq!(BotCommand::parse("foo foo foo"), None);

        // !help takes args, so shouldn't parse when none are given.
        assert_eq!(BotCommand::parse("!help"), None);

        assert_eq!(
            BotCommand::parse("!help diff.vim help things"),
            Some(BotCommand::Help {
                docs: vec!["diff.vim", "help", "things"]
            })
        );

        // Shortened version should also work
        assert_eq!(
            BotCommand::parse("!h diff.vim help things"),
            Some(BotCommand::Help {
                docs: vec!["diff.vim", "help", "things"]
            })
        );

        assert_eq!(
            BotCommand::parse("!sandwich some_person"),
            Some(BotCommand::Sandwich { to: "some_person" })
        );

        assert_eq!(
            BotCommand::parse("!sandwich some_person other_people idontgetasandwich"),
            Some(BotCommand::Sandwich { to: "some_person" })
        );
    }
}
