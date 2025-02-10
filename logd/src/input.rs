// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

use crate::{RecordSender, MAX_RECORD_SIZE, UNIX_PACKET_PATH, UNIX_STREAM_PATH};
use anyhow::{Context, Error};
use async_stream::stream;
use bytes::BytesMut;
use feo_log::{debug, info, trace};
use feo_logger::record::OwnedRecord;
use futures::{Stream, StreamExt};
use std::path::Path;
use std::{fs, io};
use tokio::{net, pin};
use tokio_seqpacket::UnixSeqpacketListener;
use tokio_util::codec::{self, FramedRead, LengthDelimitedCodec};

pub async fn packet(record_sender: RecordSender) -> Result<(), Error> {
    let path = UNIX_PACKET_PATH;
    // Check if socket is present and remove if necessary
    if Path::new(path).exists() {
        debug!("Removing stale socket at {path:?}");
        fs::remove_file(path).with_context(|| format!("failed to remove {path}"))?;
    }

    // Bind
    info!("Binding to {path:?}");
    let mut listener = UnixSeqpacketListener::bind(path)?;

    // Listen
    info!("Listening on {path:?}");
    loop {
        let socket = listener
            .accept()
            .await
            .context("failed to accept packet connection")?;
        info!("Accepted seqpacket connection");
        let stream = stream! {
            let mut buffer = [0u8; MAX_RECORD_SIZE];
            loop {
                let len = socket.recv(&mut buffer).await?;

                // Connection closed?
                if len == 0 {
                    break;
                }
                // No length prefix, so just decode the record. Seqpacket guarantees that the entire record is read.

                yield OwnedRecord::decode(&buffer[..len]);
            }
        };

        // Spawn a new task to handle the connection
        tokio::spawn(connection(stream, record_sender.clone()));
    }
}

/// Handle a connection.
pub async fn stream(record_sender: RecordSender) -> Result<(), Error> {
    let path = UNIX_STREAM_PATH;
    // Check if socket is present and remove if necessary
    let socket = Path::new(path);
    if socket.exists() {
        debug!("Removing stale socket at {path:?}");
        fs::remove_file(socket).with_context(|| format!("failed to remove {path}"))?;
    }

    // Bind
    info!("Binding to {path:?}");
    let listener =
        net::UnixListener::bind(path).with_context(|| format!("failed to bind to {path}"))?;

    // Listen
    info!("Listening on {:?}", path);
    loop {
        let (stream, addr) = listener
            .accept()
            .await
            .context("failed to accept unix connection")?;
        info!("Accepted connection from {addr:?}");
        let framed = FramedRead::with_capacity(stream, LogStreamCodec::default(), MAX_RECORD_SIZE);

        // Spawn a new task to handle the connection
        tokio::spawn(connection(framed, record_sender.clone()));
    }
}

/// Handle a connection.
async fn connection<S: Stream<Item = io::Result<OwnedRecord>>>(
    stream: S,
    record_sender: RecordSender,
) {
    pin!(stream);

    loop {
        let record = match stream.next().await {
            Some(Ok(record)) => {
                trace!("Received record: {:?}", record);
                record
            }
            Some(Err(e)) => {
                info!("Failed to read socket: {:?}. Closing connection", e);
                break;
            }
            None => {
                info!("Connection closed");
                break;
            }
        };

        record_sender.send(record).await.expect("channel closed");
    }
}

/// A codec for decoding log records from a stream. The stream contains length-prefixed records.
/// The length is a u32 in big-endian format.
#[derive(Debug)]
pub struct LogStreamCodec {
    inner: LengthDelimitedCodec,
}

impl Default for LogStreamCodec {
    fn default() -> Self {
        let inner = LengthDelimitedCodec::builder()
            .big_endian()
            .length_field_type::<u32>()
            .new_codec();
        Self { inner }
    }
}

impl codec::Decoder for LogStreamCodec {
    type Item = OwnedRecord;
    type Error = std::io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        self.inner
            .decode(src)?
            .map(|record| OwnedRecord::decode(&record))
            .transpose()
    }
}
