use bytes::Bytes;
use clap::{Parser, value_parser};
use crossbeam_channel::unbounded;
use futures::stream::StreamExt;
use log::{debug, error, info};
use reqwest::Client;
use serde::Deserialize;
use std::collections::HashMap;
use std::error::Error;
use std::fs::{File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::time::Duration;

mod types;
use types::{PGNMetadata, Time};

mod parse;
use parse::ChessParser;

/// Chess.com bulk game downloader. By default downloads all time controls and does not sort the games into different files based on time control.
#[derive(Parser, Clone)]
#[command(version = "0.4.0", name = "chess_dl", author = "Nimrod Hajaj")]

struct Options {
    #[arg(required = true)]
    usernames: Vec<String>,
    /// Output directory.
    #[arg(short, default_value("."), value_parser(value_parser!(PathBuf)))]
    output_dir: PathBuf,

    #[arg(long, display_order = 3)]
    blitz: bool,

    #[arg(long, display_order = 2)]
    bullet: bool,

    #[arg(long, display_order = 4)]
    rapid: bool,
    /// Currently unsupported and not detected by the parser.
    #[arg(long, display_order = 5)]
    daily: bool,

    /// Sort files by time control.
    #[arg(short, long, group = "time")]
    timesort: bool,

    /// Downloads raw files and does no parsing. This conflicts with any flag that depends on parsing.
    #[arg(long, conflicts_with_all(&["blitz", "bullet", "rapid", "daily", "timesort"]))]
    raw: bool,

    /// Number of download attempts for each archive.
    #[arg(short, long, default_value("8"))]
    attempts: u32,

    /// Number of concurrent downloads. Too many would cause downloads to fail, but higher is usually faster.
    #[arg(short, long, default_value("10"))]
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
    download_all_games(&options).await
}

async fn download_all_games(opt: &Options) -> Result<(), Box<dyn Error>> {
    let client = Client::new();
    let mut archives = Archives::new();

    for username in &opt.usernames {
        let archives_url = format!(
            "https://api.chess.com/pub/player/{}/games/archives",
            username
        );
        debug!("Archives URL: {}", archives_url);
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
                        url,
                    }
                })
                .collect::<Archives>()),
        );
    }

    let num_archives = archives.len();
    info!("Found {} archives to download", num_archives);

    let output_path = opt.output_dir.clone();

    let (send, rec) = unbounded::<PGNMessage>();
    let opt_cp = opt.clone();
    let write_worker =
        std::thread::spawn(move || process_pgn_messages(rec, opt_cp, output_path, num_archives));
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
                                    bytes,
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

    // Join the writer thread and handle its nested Result
    let worker_join_result = write_worker.join();

    // Handle the nested Result: first for thread panic, then for worker function error
    let final_result: Result<(), Box<dyn Error>> = match worker_join_result {
        Ok(worker_inner_result) => {
            // Thread did not panic, now handle the Result from the worker function
            worker_inner_result.map_err(|e| e as Box<dyn Error>)
        }
        Err(panic_info) => {
            // Thread panicked, convert panic info to a Box<dyn Error>
            let err_msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                format!("Worker thread panicked: {}", s)
            } else if let Some(s) = panic_info.downcast_ref::<String>() {
                format!("Worker thread panicked: {}", s)
            } else {
                "Worker thread panicked".to_string()
            };
            Err(Box::new(std::io::Error::new(std::io::ErrorKind::Other, err_msg)))
        }
    };

    // Propagate any error from either the worker function or the panic
    final_result?;

    Ok(())
}

fn process_pgn_messages(
    rec: crossbeam_channel::Receiver<PGNMessage>,
    opt: Options,             // Takes ownership of Options
    mut output_path: PathBuf, // Takes ownership of output_path
    num_archives: usize,      // Need this to know when to stop
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let mut files = HashMap::<PGNMetadata, File>::new();
    for i in 0..num_archives {
        let pgn_message = rec.recv_timeout(Duration::from_secs(120));
        match pgn_message {
            Ok(msg) => {
                if msg.bytes.is_empty() {
                    error!("Received empty PGN message for user {}", msg.username);
                    continue;
                }
                if opt.raw {
                    files
                        .entry(PGNMetadata::from_username(&msg.username))
                        .or_insert_with(|| tempfile::tempfile().unwrap())
                        .write_all(&msg.bytes)?;
                } else {
                    match ChessParser::parse(std::str::from_utf8(&msg.bytes)?) {
                        Ok(parser) => {
                            for game_result in parser {
                                match game_result {
                                    Ok(game) => {
                                        let all = !(opt.bullet | opt.blitz | opt.rapid | opt.daily);
                                        let time_allowed = match game.time {
                                            Time::Misc => all,
                                            Time::Bullet => opt.bullet || all,
                                            Time::Blitz => opt.blitz || all,
                                            Time::Rapid => opt.rapid || all,
                                            Time::Daily => opt.daily || all,
                                            Time::None => {
                                                error!(
                                                    "Unexpected Time::None encountered for a game. Skipping game."
                                                );
                                                continue;
                                            }
                                        };
                                        if time_allowed {
                                            let game_info = PGNMetadata::from_game(
                                                &msg.username,
                                                &game,
                                                !opt.timesort,
                                            );
                                            files
                                                .entry(game_info)
                                                .or_insert_with(|| {
                                                    tempfile::tempfile().expect(
                                                        "Failed to create tempfile for game.",
                                                    )
                                                })
                                                .write_all(game.pgn.as_bytes())
                                                .expect("Failed to write game PGN to tempfile.");
                                        }
                                    }
                                    Err(e) => {
                                        error!(
                                            "Error parsing a game from archive for user {}: {}",
                                            msg.username, e
                                        );
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            error!("Error parsing PGN archive for user {}: {}", msg.username, e);
                        }
                    }
                }
                debug!("Processed message {}/{}", i + 1, num_archives);
            }
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                error!(
                    "Timeout receiving PGN message after 120 seconds. This might indicate a problem with the download process."
                );
                return Err(Box::new(crossbeam_channel::RecvTimeoutError::Timeout));
            }
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                info!("Channel disconnected. Received {} archives.", i);
                return Err(Box::new(crossbeam_channel::RecvTimeoutError::Disconnected));
            }
        }
    }

    for (game_info, val) in files.iter_mut() {
        let mut tmp_file = val;
        tmp_file.seek(SeekFrom::Start(0))?;

        let output_str = format!("{}", game_info);
        output_path.set_file_name(output_str);
        let mut dest_file = OpenOptions::new()
            .write(true)
            .create(true)
            .open(&output_path)?;
        info!(
            "Copying temporary file to {}...",
            output_path.as_os_str().to_str().unwrap_or("<invalid path>")
        );
        let num_bytes = std::io::copy(&mut tmp_file, &mut dest_file)?;
        info!("Number of bytes copied: {}", num_bytes);
    }
    Ok(())
}
