use bytes::Bytes;
use clap::{CommandFactory, Parser};
use crossbeam_channel::unbounded;
use futures::Future;
use futures::stream::StreamExt;
use log::{debug, error, info};
use pin_project_lite::pin_project;
use reqwest::Client;
use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT};
use serde::Deserialize;
use std::collections::HashMap;
use std::error::Error;
use std::fs::{File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use tokio::signal::ctrl_c;
use tokio::time::{Duration as TokioDuration, sleep}; // Use different name to avoid conflict with std::time::Duration
use tokio_util::sync::CancellationToken;

mod types;

const DEFAULT_ATTEMPTS: u32 = 512;
use types::{PGNMetadata, Time};

mod parse;
use parse::ChessParser;

mod cli;
use crate::cli::Options;

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
    let options = Options::parse();

    if options.usernames.is_empty() {
        // If no subcommand is provided, usernames are required
        eprintln!("Error: The following required arguments were not provided:");
        eprintln!("  <usernames>...");
        // Use render_long_help() instead of render_usage() for more comprehensive help
        println!("\n{}", Options::command().render_long_help());
        std::process::exit(1);
    }

    let mut options_clone = options.clone();
    options_clone.usernames = options_clone
        .usernames
        .into_iter()
        .map(|u| u.to_lowercase())
        .collect::<Vec<String>>();

    let token = CancellationToken::new();

    // Spawn a task to listen for Ctrl+C and cancel the token
    let token_clone_ctrlc = token.clone();
    tokio::spawn(async move {
        if let Err(e) = ctrl_c().await {
            error!("Failed to listen for Ctrl+C: {}", e);
        } else {
            info!("Ctrl+C received. Cancelling downloads.");
            token_clone_ctrlc.cancel();
        }
    });

    if let Some(minutes) = options_clone.time_limit {
        info!("Setting total time limit to {} minutes.", minutes);
        let duration = TokioDuration::from_secs(minutes * 60);
        let token_clone_timeout = token.clone();
        tokio::spawn(async move {
            sleep(duration).await;
            info!("Time limit reached. Cancelling downloads.");
            token_clone_timeout.cancel();
        });
    }

    // Pass the token to the main download function
    download_all_games(&options_clone, token).await
}

pin_project! {
    #[project = BytesProj]
    struct CancellableBytes {
        #[pin]
        inner: futures::future::BoxFuture<'static, Result<Bytes, reqwest::Error>>,
    }
}

impl Future for CancellableBytes {
    type Output = Result<Bytes, reqwest::Error>;
    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        self.project().inner.poll(cx)
    }
}

async fn download_all_games(opt: &Options, token: CancellationToken) -> Result<(), Box<dyn Error>> {
    let client = Client::builder()
        .default_headers(
            HeaderMap::from_iter(vec![(USER_AGENT, HeaderValue::from_static("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/58.0.3029.110 Safari/537.36"))])
        )
        .build()?;
    let mut archives = Archives::new();
    let failed_archives: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let downloaded_count = Arc::new(AtomicUsize::new(0)); // Counter for successfully downloaded non-empty archives
    let total_games_count = Arc::new(AtomicUsize::new(0)); // Counter for total games processed
    let total_bytes_written = Arc::new(AtomicUsize::new(0)); // Counter for total bytes written to final files

    for username in &opt.usernames {
        let archives_url = format!(
            "https://api.chess.com/pub/player/{}/games/archives",
            username
        );
        debug!("Archives URL: {}", archives_url);
        {
            let resp = client.get(&archives_url).send().await?;
            debug!("Received status code: {}", resp.status());
            let container = resp.json::<JSONArchivesContainer>().await?;
            let mut downloaded_archives: Archives = container
                .archives
                .into_iter()
                .map(|mut url| {
                    url.push_str("/pgn");
                    Archive {
                        username: username.clone(),
                        url,
                    }
                })
                .collect();
            archives.append(&mut downloaded_archives);
        }
    }

    let total_archives_count = archives.len();
    info!("Found {} archives to download", total_archives_count);

    let output_path = opt.output_dir.clone();

    let (send, rec) = unbounded::<PGNMessage>();
    let opt_cp = opt.clone();
    // The writer thread doesn\'t need access to failed_archives Arc directly,
    // as it only receives PGN messages. The main thread handles reporting.
    let total_games_count_clone = Arc::clone(&total_games_count);
    let total_bytes_written_clone = Arc::clone(&total_bytes_written);
    let write_worker = std::thread::spawn(move || {
        process_pgn_messages(
            rec,
            opt_cp,
            output_path,
            total_games_count_clone,
            total_bytes_written_clone,
        )
    });
    let downloaded_count_clone = Arc::clone(&downloaded_count); // Clone for use in the async block
    let fetches = futures::stream::iter(archives.into_iter().map(|archive| {
        let client = &client;
        let send = send.clone();
        let failed_archives_arc = Arc::clone(&failed_archives);
        let token = token.clone(); // Capture token
        let downloaded_count_inner = Arc::clone(&downloaded_count_clone); // Clone for each future
        async move {
            for attempt in 1..DEFAULT_ATTEMPTS + 1 {
                // Check for cancellation before attempting download or retrying
                if token.is_cancelled() {
                    info!("Download for {} cancelled.", archive.url);
                    let mut failed = failed_archives_arc.lock().unwrap();
                    failed.push(archive.url.clone());
                    break; // Exit retry loop
                }
                match client.get(&archive.url).send().await {
                    Ok(resp) => {
                        if resp.status().is_success() {
                            match resp.bytes().await {
                                Ok(bytes) => {
                                    if !bytes.is_empty() {
                                        debug!(
                                            "Downloaded {} bytes from {}",
                                            bytes.len(),
                                            archive.url
                                        );
                                        downloaded_count_inner.fetch_add(1, Ordering::SeqCst); // Increment the counter
                                        send.send(PGNMessage {
                                            username: archive.username,
                                            bytes,
                                        })
                                        .expect("Send failed");
                                        break; // Success, exit retry loop
                                    } else {
                                        info!("Received empty bytes from {}. Treating as completed for this archive.", archive.url);
                                        break; // Treat empty bytes as a non-retryable completion
                                    }
                                }
                                Err(e) => {
                                    error!("Failed to read bytes from {}: {}", archive.url, e);
                                    // Fall through to retry/failure logic
                                }
                            }
                        } else {
                            error!(
                                "Received non-success status code {} from {}",
                                resp.status(),
                                archive.url
                            );
                        }
                    }
                    Err(e) => error!("Failed to download {}: {}", archive.url, e),
                }

                if attempt < DEFAULT_ATTEMPTS {
                    // Check for cancellation before sleeping
                    if token.is_cancelled() {
                        info!(
                            "Retry delay for {} skipped due to cancellation.",
                            archive.url
                        );
                        let mut failed = failed_archives_arc.lock().unwrap();
                        failed.push(archive.url.clone());
                        break; // Exit retry loop
                    }
                    // Calculate exponential backoff delay: base_delay * 2^(attempt - 1)
                    // Using a base delay of 1 second.
                    let delay = TokioDuration::from_secs(u64::pow(2, (attempt - 1) as u32));
                    info!(
                        "Attempt {}/{} failed for {}. Retrying in {:?}...",
                        attempt, DEFAULT_ATTEMPTS, archive.url, delay
                    );
                    sleep(delay).await;
                } else {
                    error!(
                        "Failed to download {} after {} attempts.",
                        archive.url, DEFAULT_ATTEMPTS
                    );
                    let mut failed = failed_archives_arc.lock().unwrap();
                    failed.push(archive.url.clone());
                }
            }
        }
    }))
    .buffer_unordered(opt.concurrent)
    .collect::<Vec<()>>();
    fetches.await;

    // Drop the sender to signal the writer thread that no more messages will be sent
    drop(send);

    info!("Main thread: Waiting for writer thread to finish...");
    // Join the writer thread and handle its nested Result
    let worker_join_result = write_worker.join();
    info!("Main thread: Writer thread finished.");

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
            Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::Other,
                err_msg,
            )))
        }
    };

    // Propagate any error from either the worker function or the panic
    final_result?;

    // Report archive statistics
    let failed = failed_archives.lock().unwrap();
    let num_failed = failed.len();
    let num_downloaded_non_empty = downloaded_count.load(Ordering::SeqCst); // Load the atomic counter
    let total_archives = total_archives_count; // Use the captured total number of archives
    let num_empty = total_archives
        .saturating_sub(num_failed)
        .saturating_sub(num_downloaded_non_empty);

    if num_failed > 0 {
        error!(
            "Failed to download {} out of {} archives:",
            num_failed, total_archives
        );
        for url in failed.iter() {
            error!("  {}", url);
        }
    }

    info!("--- Download Summary ---");
    info!("Total archives found: {}", total_archives);
    info!(
        "Successfully downloaded (non-empty): {}",
        num_downloaded_non_empty
    );
    info!("Empty archives received: {}", num_empty); // Report empty archives
    info!("Failed to download: {}", num_failed);

    let total_games = total_games_count.load(Ordering::SeqCst);
    let total_mb_written = total_bytes_written.load(Ordering::SeqCst) as f64 / 1024.0 / 1024.0;

    info!("Total games processed: {}", total_games);
    info!("Total bytes written to files: {:.2} MB", total_mb_written);

Ok(())
}

fn process_pgn_messages(
    rec: crossbeam_channel::Receiver<PGNMessage>,
    opt: Options,                          // Takes ownership of Options
    mut output_path: PathBuf,              // Takes ownership of output_path
    total_games_count: Arc<AtomicUsize>,   // Shared counter for total games
    total_bytes_written: Arc<AtomicUsize>, // Shared counter for total bytes written
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let mut files = HashMap::<PGNMetadata, File>::new();
    let mut received_count = 0; // Track how many messages were received

    // Loop until the channel is disconnected
    while let Ok(msg) = rec.recv() {
        received_count += 1;
        // We no longer signal failure with empty bytes, so this check is removed.
        // If a download truly fails after retries, it won't be sent on the channel.

        if opt.raw || (opt.group_colors && !opt.separate_time) {
            files
                .entry(PGNMetadata::from_username(&msg.username, opt.group_users))
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
                                        !opt.separate_time,
                                        opt.group_users,
                                        opt.group_colors,
                                    );
                                    files
                                        .entry(game_info)
                                        .or_insert_with(|| {
                                            tempfile::tempfile()
                                                .expect("Failed to create tempfile for game.")
                                        })
                                        .write_all(game.pgn.as_bytes()) // Writing game PGN bytes
                                        .expect("Failed to write game PGN to tempfile.");
                                    total_games_count.fetch_add(1, Ordering::SeqCst); // Increment total games count
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
        // debug!("Processed message {}/{}", received_count, num_archives); // Optional: Can't reliably track total archives received this way anymore
    }

    info!(
        "Channel disconnected. Received {} archive(s).",
        received_count
    );
    info!("Writer thread: Channel disconnected. Exiting message processing loop.");
    // If we exit the loop, it means the channel was disconnected, which is the expected signal for completion.
    // So, we don\\\'t need to return an error here.

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
        total_bytes_written.fetch_add(num_bytes as usize, Ordering::SeqCst); // Increment total bytes written
    }
    Ok(())
}
