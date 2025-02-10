// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

//! Central trace collector

use anyhow::{bail, Error};
use feo_logger::fmt::format_owned;
use feo_logger::record::OwnedRecord;
use tokio::sync::mpsc;
use tokio::task::JoinSet;

mod input;

pub const MAX_RECORD_SIZE: usize = feo_logger::MAX_RECORD_SIZE;
const RECORD_CHANNEL_SIZE: usize = 100;
pub const UNIX_PACKET_PATH: &str = "/tmp/logd.sock";
pub const UNIX_STREAM_PATH: &str = "/tmp/logd.stream.sock";

type RecordSender = mpsc::Sender<OwnedRecord>;
type RecordReceiver = mpsc::Receiver<OwnedRecord>;

/// Start tasks for each input source. Start a task that processes records.
pub async fn run() -> Result<(), Error> {
    let (record_sender, record_receiver) = mpsc::channel(RECORD_CHANNEL_SIZE);
    let mut tasks = JoinSet::new();

    tasks.spawn(process_records(record_receiver));
    tasks.spawn(input::stream(record_sender.clone()));
    tasks.spawn(input::packet(record_sender));

    let done = tasks.join_next().await.expect("no tasks to join");
    match done {
        Ok(_) => unreachable!("tasks should never return"),
        Err(e) => bail!(e),
    }
}

/// Process records. Placeholder - just print to stdout.
async fn process_records(mut record_receiver: RecordReceiver) -> Result<(), Error> {
    while let Some(record) = record_receiver.recv().await {
        format_owned(record, std::io::stdout())?;
    }
    unreachable!("record receiver closed");
}
