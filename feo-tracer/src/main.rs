// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

//! Collect trace data - placeholder

use anyhow::{bail, Context, Error};
use argh::FromArgs;
use feo_log::{debug, info, LevelFilter};
use feo_tracer::io::listen;
use feo_tracer::perfetto;
use futures::FutureExt;
use indicatif_log_bridge::LogWrapper;
use std::future::pending;
use std::path::{Path, PathBuf};
use std::{fs, io};
use tokio::sync::mpsc;
use tokio::{runtime, select, signal, task, time};

/// Progress bar wrapper
mod progress;

/// Path to the seqpacket socket
const UNIX_PACKET_PATH: &str = "/tmp/feo-tracer.sock";
/// Size of the message channel
const MESSAGE_CHANNEL_SIZE: usize = 100;

#[derive(FromArgs)]
#[argh(help_triggers("-h", "--help", "help"))]
/// Tracer arguments
struct Args {
    #[argh(description = "trace duration in seconds")]
    #[argh(option, short = 'd')]
    duration: Option<u64>,

    #[argh(description = "output path")]
    #[argh(option, short = 'o')]
    out: PathBuf,

    #[argh(description = "log level")]
    #[argh(option, short = 'l')]
    log_level: Option<LevelFilter>,
}

/// Tracer main entry point
fn main() -> Result<(), Error> {
    let Args {
        duration,
        out,
        log_level,
    } = argh::from_env();

    // Initialize logging
    let logger = feo_logger::Logger::new(true, false);

    // Initialize progress bar
    let mut progress = progress::Progress::new()?;

    // Wrap the loger in the progress bar to avoid interleaving
    LogWrapper::new(progress.bar(), logger).try_init()?;
    feo_log::set_max_level(log_level.unwrap_or(LevelFilter::Warn));

    info!("Starting feo-tracer");

    let mut tasks = task::JoinSet::new();

    let (message_sender, mut message_receiver) = mpsc::channel(MESSAGE_CHANNEL_SIZE);

    // Listen for incoming connections on a seqpacket socket
    // Forward the messages to the message channel.
    let fan_in_seqpacket = {
        let message_sender = message_sender.clone();
        async move {
            let path = Path::new(UNIX_PACKET_PATH);
            // Check if socket is present and remove if necessary
            if path.exists() {
                debug!("Removing stale socket at {path:?}");
                fs::remove_file(path).with_context(|| format!("failed to remove {path:?}"))?;
            }
            listen(path, message_sender).await
        }
    };

    // Handle incoming messages on the message channel. The channel yields
    // messages from all connected processes.
    let process_messages = {
        // Open the output file and create a progress bar for the writes
        let writer = io::BufWriter::new(
            fs::File::create(&out)
                .with_context(|| format!("failed to create {}", out.display()))?,
        );

        // Wrap writer in a progress bar
        let writer = progress.add_writer(&format!("perfetto output ({})", out.display()), writer);

        // Create a perfetto writer
        let mut perfetto = perfetto::Perfetto::new(writer);

        // Process messages as they arrive
        let process_packets = async move {
            while let Some(message) = message_receiver.recv().await {
                progress.on_packet(&message);
                perfetto.on_packet(message)?;
            }
            Ok(())
        };

        // Timeout if configured or wait indefinitely
        let timeout = async move {
            if let Some(duration) = duration.map(time::Duration::from_secs) {
                info!("Tracing for {duration:?}");
                time::sleep(duration).await;
                info!("Traced for {duration:?}. Shutting down...");
            } else {
                pending::<()>().await;
            }
        };
        let run = async move {
            select! {
                r = process_packets => r,
                _ = timeout => Ok(()),
                _ =  signal::ctrl_c() => Ok(()),
            }
        };
        run.inspect(|_| info!("Tracing complete"))
    };

    // Wait for all tasks to finish or error
    let run = async {
        tasks.spawn(fan_in_seqpacket);
        tasks.spawn(process_messages);

        match tasks.join_next().await.expect("no tasks to join") {
            Ok(_) => Ok(()),
            Err(e) => bail!(e),
        }
    };

    // Fire up runtime and wait
    runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()?
        .block_on(run)
}
