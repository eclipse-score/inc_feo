// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

//! FEO data recorder. Records communication for debugging and development purposes

use crate::ids::AgentId;
use crate::recording::registry::TypeRegistry;
use crate::recording::transcoder::ComRecTranscoder;
use crate::signalling::common::interface::ConnectRecorder;
use crate::signalling::common::signals::Signal;
use crate::timestamp;
use crate::timestamp::{timestamp, Timestamp};
use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;
use core::time::Duration;
use feo_log::{debug, error, trace};
use io::Write;
use postcard::experimental::max_size::MaxSize;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::BufWriter;
use std::{fs, io};

/// Maximum allowed length of topics and type names in the recording
const TOPIC_TYPENAME_MAX_SIZE: usize = 256;

/// The data recorder.
pub(crate) struct FileRecorder<'s> {
    // ID of the recorder
    id: AgentId,

    /// Connector to the primary
    connector: Box<dyn ConnectRecorder>,

    /// Receive timeout used on poll
    receive_timeout: Duration,

    // A file writer receiving the data
    writer: BufWriter<fs::File>,

    // Which topics with what types to record
    rules: RecordingRules,

    // The type registry
    registry: &'s TypeRegistry,

    // Transcoders reading and serializing com data
    transcoders: Vec<Box<dyn ComRecTranscoder>>,
}

impl<'s> FileRecorder<'s> {
    /// Create a new data recorder
    pub(crate) fn new<'t: 's>(
        id: AgentId,
        connector: Box<dyn ConnectRecorder>,
        receive_timeout: Duration,
        record_file: &'static str,
        rules: RecordingRules,
        registry: &'t TypeRegistry,
    ) -> io::Result<Self> {
        // Create the recording file
        let file = fs::File::create(record_file)?;
        let writer = BufWriter::new(file);

        Ok(Self {
            id,
            connector,
            receive_timeout,
            writer,
            rules,
            registry,
            transcoders: vec![],
        })
    }

    /// Run the recording
    pub(crate) fn run(&mut self) {
        // Create transcoders reading from the required topics
        debug!("Creating transcoders");
        for (topic, type_name) in self.rules.iter() {
            let info = self
                .registry
                .info_name(type_name)
                .unwrap_or_else(|| panic!("type name {type_name} not in registry"));
            let transcoder_builder = &info.comrec_builder;
            let transcoder = transcoder_builder(topic);
            debug!("Creating transcoder: {topic}, {type_name}");
            self.transcoders.push(transcoder);
        }

        debug!("Starting main loop");
        let msg_buf_size = self
            .transcoders
            .iter()
            .map(|t| t.buffer_size())
            .max()
            .unwrap_or_default();
        let mut msg_buf = vec![0; msg_buf_size];
        loop {
            // Receive the next signal from the primary process
            trace!("Waiting for next signal to record");

            let Ok(received) = self.connector.receive(self.receive_timeout) else {
                error!("Failed to receive signal, trying to continue");
                self.writer
                    .flush()
                    .unwrap_or_else(|_| error!("Failed to flush writer, trying to continue"));
                continue;
            };
            let Some(signal) = received else {
                continue;
            };
            debug!("Received signal {signal}");

            match signal {
                Signal::StartupSync(sync_info) => {
                    timestamp::initialize_from(sync_info);
                }
                // If received a step signal, or an end-of-taskchain signal,
                // record the current latest change of com data, then record the signal.
                // Also, flush the recording file at whenever the end of the task chain is reached.
                Signal::Step(_) => {
                    self.record_com_data(&mut msg_buf);
                    self.record_signal(signal);
                }
                Signal::TaskChainEnd(_) => {
                    self.record_com_data(&mut msg_buf);
                    self.record_signal(signal);
                    self.flush();
                    self.send_recorder_ready();
                }

                // Otherwise, only record the signal
                _ => {
                    self.record_signal(signal);
                }
            }
        }
    }

    /// Flush the recording file
    fn flush(&mut self) {
        if let Err(e) = self.writer.flush() {
            panic!("failed to flush recording file: {e}");
        }
    }

    // Record the latest changes of com data
    fn record_com_data(&mut self, data_buffer: &mut [u8]) {
        for transcoder in self.transcoders.iter() {
            let data = transcoder.read_transcode(data_buffer);
            if let Some(serialized_data) = data {
                // create serialized data description record
                assert!(
                    transcoder.type_name().len() <= TOPIC_TYPENAME_MAX_SIZE,
                    "serialized type name exceeds maximal size of {TOPIC_TYPENAME_MAX_SIZE}"
                );
                assert!(
                    transcoder.topic().len() <= TOPIC_TYPENAME_MAX_SIZE,
                    "serialized type name exceeds maximal size of {TOPIC_TYPENAME_MAX_SIZE}"
                );
                let description = DataDescriptionRecord {
                    timestamp: timestamp(),
                    type_name: transcoder.type_name(),
                    data_size: serialized_data.len(),
                    topic: transcoder.topic(),
                };
                let data_desc_record = Record::DataDescription(description);
                let mut buf = [0u8; Record::POSTCARD_MAX_SIZE];
                let serialized_header =
                    postcard::to_slice(&data_desc_record, &mut buf).expect("serialization failed");

                trace!("Writing data: {description:?}");

                // Write description record and subsequent data block
                // In case of failure, log an error message and continue
                // (which may result in a corrupted file)
                if let Err(e) = self
                    .writer
                    .write_all(serialized_header)
                    .and_then(|_| self.writer.write_all(serialized_data))
                {
                    error!("Failed to write data: {e:?}");
                }
            }
        }
    }

    /// Record the given signal
    fn record_signal(&mut self, signal: Signal) {
        let signal_record = Record::Signal(SignalRecord {
            signal,
            timestamp: timestamp(),
        });
        let mut buf = [0u8; Record::POSTCARD_MAX_SIZE];
        let serialized =
            postcard::to_slice(&signal_record, &mut buf).expect("serialization failed");
        if let Err(e) = self.writer.write_all(serialized) {
            error!("Failed to write signal {signal:?}: {e:?}");
        }
    }

    // Send RecorderReady signal to the primary agent
    fn send_recorder_ready(&mut self) {
        let signal = Signal::RecorderReady((self.id, timestamp()));
        self.connector
            .send_to_scheduler(&signal)
            .unwrap_or_else(|e| panic!("failed to send 'recorder_ready': {:?}", e));
    }
}

impl Drop for FileRecorder<'_> {
    fn drop(&mut self) {
        // Try to flush pending data.
        self.flush();
    }
}

/// Set of recording rules
///
/// Maps every topic to be recorded to a corresponding type name from the type registry
pub type RecordingRules = HashMap<&'static str, &'static str>;

/// Possible records in the recording file
#[derive(Debug, Serialize, Deserialize, MaxSize)]
pub enum Record<'s> {
    Signal(SignalRecord),
    #[serde(borrow)]
    DataDescription(DataDescriptionRecord<'s>),
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize, MaxSize)]
pub struct SignalRecord {
    // The monotonic time at the moment of recording as duration since the epoch
    pub timestamp: Timestamp,
    // The recorded signal
    pub signal: Signal,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub struct DataDescriptionRecord<'s> {
    // The monotonic time at the moment of recording as duration since the epoch
    pub timestamp: Timestamp,
    /// size of the appended data
    pub data_size: usize,
    #[serde(borrow)]
    /// restricted to 256 chars
    pub type_name: &'s str,
    #[serde(borrow)]
    /// restricted to 256 chars
    pub topic: &'s str,
}

impl MaxSize for DataDescriptionRecord<'_> {
    const POSTCARD_MAX_SIZE: usize = Timestamp::POSTCARD_MAX_SIZE +
        usize::POSTCARD_MAX_SIZE + // data_size
        2*( // type_name, topic
            usize::POSTCARD_MAX_SIZE + // len
                TOPIC_TYPENAME_MAX_SIZE * u8::POSTCARD_MAX_SIZE // restrict to 256 bytes
        );
}

#[cfg(test)]
mod test {
    use super::*;
    use alloc::string::String;
    use core::time::Duration;

    #[test]
    fn test_max_size_for_data_description_record() {
        let s = String::from_utf8(vec![b'a'; TOPIC_TYPENAME_MAX_SIZE]).expect("valid string");
        let record = DataDescriptionRecord {
            timestamp: Timestamp(Duration::MAX),
            data_size: usize::MAX,
            type_name: &s,
            topic: &s,
        };
        let mut buf = [0u8; DataDescriptionRecord::POSTCARD_MAX_SIZE];
        postcard::to_slice(&record, &mut buf).expect("should fit");
    }
}
