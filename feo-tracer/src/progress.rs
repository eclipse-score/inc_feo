// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

use anyhow::Error;
use feo_tracer::data;
use indicatif::{MultiProgress, ProgressStyle};
use std::collections::HashMap;
use std::io::Write;
use tokio::{task, time};

/// Progress bar template for connected clients
const MAIN_TEMPLATE: &str = "{spinner:.bold.dim} {prefix:.bold}: {decimal_bytes} at {decimal_bytes_per_sec} duration: {elapsed_precise}";
/// Progress bar template for connected clients
const CLIENT_TEMPLATE: &str = "{spinner:.bold.dim} {prefix:.bold}: {human_pos} samples at {per_sec} duration: {elapsed_precise}";
/// Connected tick chars
const TICK_CHARS: &str = "⠁⠉⠙⠚⠒⠂⠂⠒⠲⠴⠤⠄⠄⠤⠴⠲⠒⠂⠂⠒⠚⠙⠉⠁";
/// Progress bar template for disconnected clients
const DISCONNECTED_TEMPLATE: &str = "⏹ {prefix:.bold}: Received {human_pos} samples";
/// Disconnected removal delay
const DISCONNECTED_REMOVAL_DELAY: time::Duration = time::Duration::from_secs(3);

/// Progress bar
#[derive(Clone)]
pub struct Progress {
    bar: MultiProgress,
    /// Map of process id to progress bar
    connections: HashMap<u32, indicatif::ProgressBar>,
    /// Style for main progress bar
    main_style: ProgressStyle,
    /// Style for connected clients progress bars
    client_style: ProgressStyle,
    /// Disconnected style
    style_disconnected: ProgressStyle,
}

impl Progress {
    pub fn new() -> Result<Self, Error> {
        let bar = MultiProgress::new();
        let main_style = ProgressStyle::with_template(MAIN_TEMPLATE)?.tick_chars(TICK_CHARS);
        let client_style = ProgressStyle::with_template(CLIENT_TEMPLATE)?.tick_chars(TICK_CHARS);
        let style_disconnected = ProgressStyle::with_template(DISCONNECTED_TEMPLATE)?;
        let connections = HashMap::new();

        Ok(Progress {
            bar,
            connections,
            main_style,
            client_style,
            style_disconnected,
        })
    }

    /// Return a copy of the multi progress bar
    pub fn bar(&self) -> MultiProgress {
        self.bar.clone()
    }

    /// Add a writer to the progress bar
    pub fn add_writer(&mut self, name: &str, writer: impl Write) -> impl Write {
        let pb = indicatif::ProgressBar::new(0)
            .with_prefix(name.to_string())
            .with_style(self.main_style.clone());

        // Enable the steady tick
        pb.enable_steady_tick(time::Duration::from_secs(1));
        // Register the progress bar at the multi progress bar
        self.bar.add(pb.clone());

        // Create a writer that wraps the progress bar
        pb.wrap_write(writer)
    }

    /// Handle a trace packet
    pub fn on_packet(&mut self, packet: &data::TraceRecord) {
        let id = packet.process.id;
        match packet.data {
            data::RecordData::Exec => {
                let name = if let Some(name) = packet.process.name.as_ref() {
                    format!("client ({name}:{id:x})")
                } else {
                    format!("client ({id:x})")
                };
                // Create a new progress bar for the client
                let pb = indicatif::ProgressBar::new(0)
                    .with_prefix(name)
                    .with_style(self.client_style.clone());
                // Add the progress bar to the connection map
                self.connections.insert(id, pb.clone());
                // Register the progress bar at the multi progress bar
                self.bar.add(pb);
            }
            data::RecordData::Exit => {
                let Some(pb) = self.connections.get_mut(&id) else {
                    return;
                };
                let bar = self.bar.clone();
                let pb = pb.clone();
                let style = self.style_disconnected.clone();
                // Remove the progress bar after 5 seconds
                task::spawn(async move {
                    pb.set_style(style);
                    pb.tick();
                    time::sleep(DISCONNECTED_REMOVAL_DELAY).await;
                    pb.finish();
                    bar.remove(&pb);
                });
            }
            _ => {
                if let Some(pb) = self.connections.get(&id) {
                    pb.inc(1);
                }
            }
        }
    }
}
