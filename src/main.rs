use bytes::Bytes;
use clap::Clap;
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
#[clap(version = "1.0", name = "chess_dl", author = "Nimrod Hajaj")]
struct Options {
    username: String,

    #[clap(parse(from_os_str))]
    output_dir: PathBuf,

    #[clap(long)]
    blitz: bool,

    #[clap(long)]
    bullet: bool,

    #[clap(long)]
    rapid: bool,

    #[clap(long)]
    daily: bool,

    #[clap(short, long, default_value("5"))]
    attempts: i32,
}

type Archives = Vec<String>;

struct PGNMessage(Bytes);

#[derive(Deserialize, Debug)]
struct JSONArchivesContainer {
    archives: Archives,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let options = Options::parse();
    info!(
        "Downloading games of {} to file(s) {}/{}_*.pgn...",
        options.username,
        options.output_dir.as_os_str().to_str().unwrap(),
        options.username
    );
    download_all_games(&options).await?;
    Ok(())
}

async fn download_all_games(opt: &Options) -> Result<(), Box<dyn Error>> {
    let client = Client::new();
    let archives_url = format!(
        "https://api.chess.com/pub/player/{}/games/archives",
        opt.username
    );
    let archives = client
        .get(archives_url)
        .send()
        .await?
        .json::<JSONArchivesContainer>()
        .await?
        .archives;
    let num_archives = archives.len();
    info!("Found {} archives to download", num_archives);

    let mut output_path = opt.output_dir.clone();

    let (send, rec) = unbounded::<PGNMessage>();
    let opt_cp = opt.clone();
    let write_worker = std::thread::spawn(move || {
        let mut files = HashMap::<GameInfo, (File, String)>::new();
        for _ in 0..num_archives {
            let pgn = rec.recv_timeout(Duration::from_secs(120)).unwrap().0;
            let s = std::str::from_utf8(&*pgn).unwrap();
            for game in ChessParser::parse(s) {
                let game_info = GameInfo::from_game(&opt_cp.username, &game);
                let time_allowed = match game_info.time {
                    Some(Time::BULLET) => opt_cp.bullet,
                    Some(Time::BLITZ) => opt_cp.blitz,
                    Some(Time::RAPID) => opt_cp.rapid,
                    Some(Time::DAILY) => opt_cp.daily,
                    None => false,
                };
                if time_allowed {
                    let mut tmp_file = &files
                        .entry(game_info)
                        .or_insert_with(|| (tempfile::tempfile().unwrap(), opt_cp.username.clone()))
                        .0;
                    tmp_file.write_all(game.pgn.as_bytes()).unwrap();
                }
            }
        }

        for (game_info, val) in files.iter_mut() {
            let mut tmp_file = &val.0;
            tmp_file.seek(SeekFrom::Start(0)).expect("Seek failed");

            let output_str = format!(
                "{}_{}_{}.pgn",
                val.1,
                game_info.color,
                game_info.time.unwrap()
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
    let fetches = futures::stream::iter(archives.into_iter().map(|mut pgn_url| {
        let client = &client;
        let send = send.clone();
        async move {
            pgn_url.push_str("/pgn");
            for attempt in 1..opt.attempts + 1 {
                match client.get(&pgn_url).send().await {
                    Ok(resp) => match resp.bytes().await {
                        Ok(bytes) => {
                            if bytes.is_empty() {
                                if attempt == opt.attempts {
                                    error!(
                                        "Failed to download {} {}/{} times",
                                        pgn_url, attempt, opt.attempts
                                    );
                                    send.send(PGNMessage(Bytes::from(""))).expect("Send failed");
                                } else {
                                    error!(
                                        "Failed to download {} {}/{} times. Retrying...",
                                        pgn_url, attempt, opt.attempts
                                    );
                                }
                            } else {
                                info!("Downloaded {} bytes from {}", bytes.len(), pgn_url);
                                send.send(PGNMessage(bytes)).expect("Send failed");
                                break;
                            }
                        }
                        Err(_) => error!("Failed to download  {}", pgn_url),
                    },
                    Err(_) => error!("Failed to {}", pgn_url),
                }
                info!("Retrying...");
            }
        }
    }))
    .buffer_unordered(10)
    .collect::<Vec<()>>();
    fetches.await;
    write_worker.join().expect("Join failed");
    Ok(())
}
