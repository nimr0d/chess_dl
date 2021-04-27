use bytes::Bytes;
use crossbeam_channel::unbounded;
use futures::stream::StreamExt;
use log::{info, error};
use reqwest::Client;
use serde::Deserialize;
use std::error::Error;
use std::fs::OpenOptions;
use std::io::{Seek, SeekFrom, Write};
use std::time::Duration;
use std::path::PathBuf;

type Archives = Vec<String>;

struct PGNMessage(Bytes);

#[derive(Deserialize, Debug)]
struct JSONArchivesContainer {
    archives: Archives,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let args: Vec<String> = std::env::args().collect();
    let mut output = PathBuf::from(&args[2]);
    output.push(&args[1]);
    output.set_extension("pgn");
    info!("Downloading games of {} to file {}...", &args[1],output.as_os_str().to_str().unwrap());
    download_all_games(&args[1], output, 5).await?;
    Ok(())
}

async fn download_all_games(
    username: &str,
    output_path: PathBuf,
    attempts: i32,
) -> Result<(), Box<dyn Error>> {
    let client = Client::new();
    let archives_url = format!(
        "https://api.chess.com/pub/player/{}/games/archives",
        username
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
    let mut dest_file = OpenOptions::new()
        .write(true)
        .create(true)
        .open(&output_path)?;
    let mut tmp_file = tempfile::tempfile()?;
    let (send, rec) = unbounded::<PGNMessage>();
    let write_worker = std::thread::spawn(move || {
        for _ in 0..num_archives {
            let pgn = rec.recv_timeout(Duration::from_secs(120)).unwrap().0;
            tmp_file.write_all(&pgn).unwrap();
        }
        tmp_file.seek(SeekFrom::Start(0)).expect("Seek failed");
        info! ("Copying temporary file to {}...", output_path.as_os_str().to_str().unwrap());
        let num_bytes = std::io::copy(&mut tmp_file, &mut dest_file).unwrap();
        info!("Number of bytes copied: {}", num_bytes);
        drop(rec);
    });
    let fetches = futures::stream::iter(archives.into_iter().map(|mut pgn_url| {
        let client = &client;
        let send = send.clone();
        async move {
            pgn_url.push_str("/pgn");
            for attempt in 1..attempts + 1 {
                match client.get(&pgn_url).send().await {
                    Ok(resp) => match resp.bytes().await {
                        Ok(bytes) => {
                            if bytes.is_empty() {
                                if attempt == attempts {
                                    error!("Failed to download {} {}/{} times", pgn_url, attempt, attempts);
                                    send.send(PGNMessage(Bytes::from(""))).expect("Send failed");
                                } else {
                                    error!("Failed to download {} {}/{} times. Retrying...", pgn_url, attempt, attempts);
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
