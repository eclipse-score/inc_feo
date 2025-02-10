// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

//! FEO data recorder. Records communication for debugging and development purposes

use crate::recording::registry::TypeRegistry;
use crate::recording::transcoder::ComRecTranscoder;
use crate::signalling::{AgentId, MioSocketReceiver, MioSocketSender, Receiver, Sender, Signal};
use crate::timestamp::{timestamp, Timestamp};
use crate::{agent, timestamp};
use feo_log::{debug, error, info, trace};
use io::Write;
use mio::net::TcpStream;
use mio::{Events, Poll};
use postcard::experimental::max_size::MaxSize;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::BufWriter;
use std::net::SocketAddr;
use std::{fs, io};

/// Maximum allowed length of topics and type names in the recording
const TOPIC_TYPENAME_MAX_SIZE: usize = 256;

/// The data recorder.
pub struct Recorder<'s> {
    // ID of the recorder
    local_agent_id: AgentId,

    // Socket address of the primary process
    primary: SocketAddr,

    // A file writer receiving the data
    writer: BufWriter<fs::File>,

    // Which topics with what types to record
    rules: RecordingRules,

    // The type registry
    registry: &'s TypeRegistry,

    // The TCP stream receiving events to record
    recorder_stream: Option<TcpStream>,

    // The TCP stream sending ready signals
    ready_stream: Option<TcpStream>,

    // Poll object for polling the TCP stream
    poll: Poll,

    // Events object to use with the Poll object
    events: Events,

    // Transcoders reading and serializing com data
    transcoders: Vec<Box<dyn ComRecTranscoder>>,
}

impl<'s> Recorder<'s> {
    /// Create a new data recorder
    pub fn new<'t: 's>(
        local_agent_id: AgentId,
        primary: SocketAddr,
        record_file: &'static str,
        rules: RecordingRules,
        registry: &'t TypeRegistry,
    ) -> io::Result<Self> {
        // Create the recording file
        let file = fs::File::create(record_file)?;
        let writer = BufWriter::new(file);

        // Create poller and events object
        let poll = Poll::new()?;
        let events = Events::with_capacity(1024);

        Ok(Self {
            local_agent_id,
            primary,
            writer,
            rules,
            registry,
            recorder_stream: None,
            ready_stream: None,
            poll,
            events,
            transcoders: vec![],
        })
    }

    /// Run the recording
    pub fn run(&mut self) {
        self.connect_primary();

        // Create socket signal receiver and register it with the poller
        let recorder_stream = self
            .recorder_stream
            .as_mut()
            .expect("recorder signal stream not available");
        let mut receiver =
            MioSocketReceiver::new(recorder_stream, &mut self.poll, &mut self.events);
        receiver.register(0).unwrap();

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
            let signal_pdu = receiver.recv().expect("failed to receive");
            let Ok(signal) = signal_pdu.try_into() else {
                error!("Failed to decode signal pdu, trying to continue");
                self.writer
                    .flush()
                    .unwrap_or_else(|_| error!("Failed to flush writer, trying to continue"));
                continue;
            };
            debug!("Received signal {signal}");

            match signal {
                // If received a step signal, or an end-of-taskchain signal,
                // record the current latest change of com data, then record the signal.
                // Also, flush the recording file at whenever the end of the task chain is reached.
                Signal::Step(_) => {
                    Self::record_com_data(&mut self.transcoders, &mut self.writer, &mut msg_buf);
                    Self::record_signal(signal, &mut self.writer);
                }
                Signal::TaskChainEnd(_) => {
                    Self::record_com_data(&mut self.transcoders, &mut self.writer, &mut msg_buf);
                    Self::record_signal(signal, &mut self.writer);
                    Self::flush(&mut self.writer);
                    Self::send_recorder_ready(self.local_agent_id, self.ready_stream.as_mut());
                }

                // Otherwise, only record the signal
                _ => {
                    Self::record_signal(signal, &mut self.writer);
                }
            }
        }
    }

    /// Set up the event recording stream to the primary agent
    pub fn connect_primary(&mut self) {
        let (mut recorder_stream, ready_stream) =
            agent::secondary::connect_to_primary(self.local_agent_id, self.primary);

        let mut sender = MioSocketSender::new(&mut recorder_stream);
        let hello_recorder = Signal::HelloTrigger(self.local_agent_id);
        sender
            .send(&hello_recorder)
            .unwrap_or_else(|e| panic!("failed to send 'hello_recorder': {:?}", e));

        self.sync_time(&mut recorder_stream);
        info!("Time synchronization with primary agent done");

        self.recorder_stream = Some(recorder_stream);
        self.ready_stream = Some(ready_stream);
    }

    /// Wait for synchronization event from primary agent and do time synchronization
    fn sync_time(&mut self, recorder_stream: &mut TcpStream) {
        // Create socket signal receiver and register it with the poller
        let mut receiver =
            MioSocketReceiver::new(recorder_stream, &mut self.poll, &mut self.events);
        receiver.register(0).unwrap();

        // Wait until signal received
        debug!("Waiting for startup synchronization signal");
        let signal: Signal = receiver
            .recv()
            .expect("failed to receive")
            .try_into()
            .expect("failed to decode signal pdu");
        debug!("Received signal {signal}");

        // Extract synchronization info or panic, if signal is incorrect
        let sync_info = match signal {
            Signal::StartupSync(info) => info,
            _ => panic!("received unexpected signal {signal}"),
        };

        // Deregister receiver from poller
        receiver
            .deregister()
            .expect("failed to deregister receiver");

        // Synchronize from received data
        timestamp::initialize_from(sync_info);
    }

    /// Flush the recording file
    fn flush(writer: &mut BufWriter<fs::File>) {
        let result = writer.flush();
        if result.is_err() {
            panic!("failed to flush recording file");
        }
    }

    // Record the latest changes of com data
    fn record_com_data(
        transcoders: &mut Vec<Box<dyn ComRecTranscoder>>,
        writer: &mut BufWriter<fs::File>,
        data_buffer: &mut [u8],
    ) {
        for transcoder in transcoders.iter() {
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
                if let Err(e) = writer
                    .write_all(serialized_header)
                    .and_then(|_| writer.write_all(serialized_data))
                {
                    error!("Failed to write data: {e:?}");
                }
            }
        }
    }

    /// Record the given signal
    fn record_signal(signal: Signal, writer: &mut BufWriter<fs::File>) {
        let signal_record = Record::Signal(SignalRecord {
            signal,
            timestamp: timestamp(),
        });
        let mut buf = [0u8; Record::POSTCARD_MAX_SIZE];
        let serialized =
            postcard::to_slice(&signal_record, &mut buf).expect("serialization failed");
        if let Err(e) = writer.write_all(serialized) {
            error!("Failed to write signal {signal:?}: {e:?}");
        }
    }

    // Send RecorderReady signal to the primary agent
    fn send_recorder_ready(agent_id: AgentId, ready_stream: Option<&mut TcpStream>) {
        let ready_stream = ready_stream.expect("missing TCP stream");
        let mut sender = MioSocketSender::new(ready_stream);
        let signal = Signal::RecorderReady((agent_id, timestamp()));
        sender
            .send(&signal)
            .unwrap_or_else(|e| panic!("failed to send 'recorder_ready': {:?}", e));
    }
}

impl Drop for Recorder<'_> {
    fn drop(&mut self) {
        // Try to flush pending data.
        Self::flush(&mut self.writer);
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
    use super::{DataDescriptionRecord, MaxSize, Timestamp, TOPIC_TYPENAME_MAX_SIZE};
    use std::time::Duration;
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
