use bytes::Bytes;
use clap::{ArgGroup, Clap};
use crossbeam_channel::unbounded;
use futures::stream::StreamExt;
use log::{error, info};
use reqwest::Client;
use serde::Deserialize;
use std::collections::HashMap;
use std::error::Error;
use std::fs::{File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::time::Duration;

mod types;
use types::{GameInfo, Time};

mod parse;
use parse::ChessParser;

#[derive(Clap, Clone)]
#[clap(group = ArgGroup::new("time").required(true), version = "0.3.1", name = "chess_dl", author = "Nimrod Hajaj")]
/// Chess.com bulk game downloader.
struct Options {
    #[clap(required = true)]
    usernames: Vec<String>,
    /// Output directory.
    #[clap(short, default_value("."), parse(from_os_str))]
    output_dir: PathBuf,

    #[clap(long, group = "time")]
    blitz: bool,

    #[clap(long, group = "time")]
    bullet: bool,

    #[clap(long, group = "time")]
    rapid: bool,
    /// Currently unsupported and not distinguished from rapid
    #[clap(long, group = "time")]
    daily: bool,

    /// All time controls. This includes time controls that failed to parse into one of four time control categories. This does not sort by time controls.
    #[clap(long, group = "time", conflicts_with_all(&["blitz", "bullet", "rapid", "daily"]))]
    all: bool,

    /// Number of download attempts for each archive.
    #[clap(short, long, default_value("5"))]
    attempts: i32,

    /// Number of concurrent downloads. Too many would cause downloads to fail, but higher is usually faster.
    #[clap(short, long, default_value("10"))]
    concurrent: usize,
}

struct Archive {
    username: String,
    url: String,
}
type Archives = Vec<Archive>;

struct PGNMessage {
    username: String,
    bytes: Bytes,
}

#[derive(Deserialize, Debug)]
struct JSONArchivesContainer {
    archives: Vec<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let mut options = Options::parse();
    options.usernames = options
        .usernames
        .into_iter()
        .map(|u| u.to_lowercase())
        .collect::<Vec<String>>();
    download_all_games(&options).await?;
    Ok(())
}

async fn download_all_games(opt: &Options) -> Result<(), Box<dyn Error>> {
    let client = Client::new();
    let mut archives = Archives::new();

    for username in &opt.usernames {
        let archives_url = format!(
            "https://api.chess.com/pub/player/{}/games/archives",
            username
        );
        archives.append(
            &mut (client
                .get(archives_url)
                .send()
                .await?
                .json::<JSONArchivesContainer>()
                .await?
                .archives
                .into_iter()
                .map(|mut url| {
                    url.push_str("/pgn");
                    Archive {
                        username: username.clone(),
                        url: url,
                    }
                })
                .collect::<Archives>()),
        );
    }

    let num_archives = archives.len();
    info!("Found {} archives to download", num_archives);

    let mut output_path = opt.output_dir.clone();

    let (send, rec) = unbounded::<PGNMessage>();
    let opt_cp = opt.clone();
    let write_worker = std::thread::spawn(move || {
        let mut files = HashMap::<GameInfo, File>::new();
        for _ in 0..num_archives {
            let pgn_message = rec.recv_timeout(Duration::from_secs(120)).unwrap();
            let s = std::str::from_utf8(&*pgn_message.bytes).unwrap();
            for game in ChessParser::parse(s) {
                let game_info = GameInfo::from_game(&pgn_message.username, &game);
                let time_allowed = match game_info.time {
                    Some(Time::BULLET) => opt_cp.bullet || opt_cp.all,
                    Some(Time::BLITZ) => opt_cp.blitz || opt_cp.all,
                    Some(Time::RAPID) => opt_cp.rapid || opt_cp.all,
                    Some(Time::DAILY) => opt_cp.daily || opt_cp.all,
                    None => opt_cp.all,
                    Some(Time::ALL) => unreachable!(),
                };
                if time_allowed {
                    let tmp_file = files
                        .entry(game_info)
                        .or_insert_with(|| tempfile::tempfile().unwrap());
                    tmp_file.write_all(game.pgn.as_bytes()).unwrap();
                }
            }
        }

        for (game_info, val) in files.iter_mut() {
            let mut tmp_file = val;
            tmp_file.seek(SeekFrom::Start(0)).expect("Seek failed");

            let output_str = format!(
                "{}_{}_{}.pgn",
                game_info.username,
                if opt_cp.all {
                    Time::ALL
                } else {
                    game_info.time.unwrap()
                },
                game_info.color,
            );
            output_path.set_file_name(output_str);
            let mut dest_file = OpenOptions::new()
                .write(true)
                .create(true)
                .open(&output_path)
                .expect("Failed to create destination file");
            info!(
                "Copying temporary file to {}...",
                output_path.as_os_str().to_str().unwrap()
            );
            let num_bytes = std::io::copy(&mut tmp_file, &mut dest_file)
                .expect("Failed to copy to destination file");
            info!("Number of bytes copied: {}", num_bytes);
        }
        drop(rec);
    });
    let fetches = futures::stream::iter(archives.into_iter().map(|archive| {
        let client = &client;
        let send = send.clone();
        async move {
            for attempt in 1..opt.attempts + 1 {
                match client.get(&archive.url).send().await {
                    Ok(resp) => match resp.bytes().await {
                        Ok(bytes) => {
                            if bytes.is_empty() {
                                if attempt == opt.attempts {
                                    error!(
                                        "Failed to download {} {}/{} times",
                                        archive.url, attempt, opt.attempts
                                    );
                                    send.send(PGNMessage {
                                        username: archive.username.clone(),
                                        bytes: Bytes::from(""),
                                    })
                                    .expect("Send failed");
                                } else {
                                    error!(
                                        "Failed to download {} {}/{} times. Retrying...",
                                        archive.url, attempt, opt.attempts
                                    );
                                }
                            } else {
                                info!("Downloaded {} bytes from {}", bytes.len(), archive.url);
                                send.send(PGNMessage {
                                    username: archive.username,
                                    bytes: bytes,
                                })
                                .expect("Send failed");
                                break;
                            }
                        }
                        Err(_) => error!("Failed to download  {}", archive.url),
                    },
                    Err(_) => error!("Failed to {}", archive.url),
                }
                info!("Retrying...");
            }
        }
    }))
    .buffer_unordered(opt.concurrent)
    .collect::<Vec<()>>();
    fetches.await;
    write_worker.join().expect("Join failed");
    Ok(())
}
