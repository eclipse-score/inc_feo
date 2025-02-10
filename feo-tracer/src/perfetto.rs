// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

use crate::data::{TraceData, TracePacket, Value};
use anyhow::{anyhow, bail, Error};
use feo_log::info;
use perfetto_model as idl;
use prost::Message as ProstMessage;
use std::collections::HashMap;
use std::io;
use std::time::UNIX_EPOCH;

/// Sequence id for a trace. This is unique per trace.
type SequenceId = u32;
/// Track uuid for a trace. This is unique per trace.
type TrackUuid = u64;

/// Span
#[derive(Debug, Default)]
struct Span {
    /// Thread group name in which the span was created.
    pid: u32,
    /// Trace of the span.
    trace: idl::Trace,
    /// Attributes of the span.
    attributes: Value,
}

impl Span {
    /// Create a new span.
    fn new(pid: u32, trace: idl::Trace, attributes: Value) -> Self {
        Self {
            pid,
            trace,
            attributes,
        }
    }
}

/// Perfetto writer
pub struct Perfetto<W> {
    writer: (W, u64),
    spans: HashMap<(u32, u64), Span>,
    track_uuid: TrackUuid,
    sequence_id: SequenceId,
}

impl<W> Drop for Perfetto<W> {
    fn drop(&mut self) {
        info!(
            "Dropping perfetto writer. Wrote {} bytes",
            human_bytes::human_bytes(self.writer.1 as f64)
        );
    }
}

impl<W: io::Write> Perfetto<W> {
    pub fn new(writer: W) -> Self {
        let spans = HashMap::new();
        let track_uuid = rand::random();
        let sequence_id = rand::random();

        Self {
            writer: (writer, 0),
            spans,
            track_uuid,
            sequence_id,
        }
    }

    pub fn on_packet(&mut self, message: TracePacket) -> Result<(), Error> {
        let pid = message.process.id;
        let process = message.process;
        let thread = message.thread;
        let timestamp_nanos = message.timestamp.duration_since(UNIX_EPOCH)?.as_nanos() as u64;

        // Map record to event. This is unfortunate not possible directly in the match bel
        // below because the types of the fields differ.
        let data = match message.data {
            TraceData::Record { id, event } => TraceData::Event {
                parent_span: Some(id),
                event,
            },
            data => data,
        };

        match data {
            TraceData::Exec => (),
            TraceData::Exit => {
                // Remove all spans that belong to the process
                self.spans.retain(|_, span| span.pid != pid);
            }
            TraceData::NewSpan { id, attributes } => {
                let key = (pid, id);
                assert!(!self.spans.contains_key(&key));

                let trace = {
                    // There's the process, thread, and the span itself
                    let mut packet = Vec::with_capacity(4);
                    let thread = thread.expect("missing thread info in new span");
                    packet.push(self.process_descriptor(pid, process.name.as_deref()));
                    packet.push(self.thread_descriptor(pid, thread.id, thread.name.as_deref()));
                    idl::Trace { packet }
                };

                self.spans.insert(key, Span::new(pid, trace, attributes));
            }
            TraceData::EnterSpan { id } => {
                let sequence_id = self.sequence_id();
                let Some(span) = self.spans.get_mut(&(pid, id)) else {
                    return Ok(());
                };

                let Some(tid) = thread.map(|t| t.id) else {
                    bail!("missing thread info in enter span");
                };
                let span_name = metadata_name(&span.attributes);
                let location = metadata_location(&span.attributes);
                let debug_annotations = debug_annotations(&span.attributes);
                let thread_track_uuid = tid;
                let event = create_event(
                    thread_track_uuid as u64,
                    span_name,
                    location,
                    debug_annotations,
                    Some(idl::track_event::Type::SliceBegin),
                );
                let packet = idl::TracePacket {
                    data: Some(idl::trace_packet::Data::TrackEvent(event)),
                    timestamp: Some(timestamp_nanos),
                    trusted_pid: Some(pid as _),
                    optional_trusted_packet_sequence_id: Some(sequence_id),
                    ..Default::default()
                };

                span.trace.packet.push(packet);
            }
            TraceData::ExitSpan { id } => {
                let key = (pid, id);
                let Some(mut span) = self.spans.remove(&key) else {
                    return Ok(());
                };
                let Some(tid) = thread.map(|t| t.id) else {
                    return Err(anyhow!("missing thread info in exit span"));
                };

                let span_name = metadata_name(&span.attributes);
                let location = metadata_location(&span.attributes);
                let debug_annotations = debug_annotations(&span.attributes);
                let event = create_event(
                    tid as u64,
                    span_name,
                    location,
                    debug_annotations,
                    Some(idl::track_event::Type::SliceEnd),
                );
                let packet = idl::TracePacket {
                    data: Some(idl::trace_packet::Data::TrackEvent(event)),
                    timestamp: Some(timestamp_nanos),
                    trusted_pid: Some(pid as _),
                    optional_trusted_packet_sequence_id: Some(self.sequence_id()),
                    ..Default::default()
                };

                span.trace.packet.push(packet);

                // Flush
                self.append(&span.trace)?;
            }

            TraceData::Record { .. } => unreachable!(),
            TraceData::Event { parent_span, event } => {
                let Some(tid) = thread.as_ref().map(|t| t.id) else {
                    bail!("missing thread info in exit span");
                };
                let name = metadata_name(&event);
                let location = metadata_location(&event);
                let debug_annotations = debug_annotations(&event);
                let track_event = create_event(
                    tid as u64,
                    name,
                    location,
                    debug_annotations,
                    Some(idl::track_event::Type::Instant),
                );
                let packet = perfetto_model::TracePacket {
                    data: Some(idl::trace_packet::Data::TrackEvent(track_event)),
                    trusted_pid: Some(pid as _),
                    timestamp: Some(timestamp_nanos),
                    optional_trusted_packet_sequence_id: Some(self.sequence_id()),
                    ..Default::default()
                };

                // If the event is associated with a span, append to the span.
                if let Some(span) = parent_span.and_then(|id| self.spans.get_mut(&(pid, id))) {
                    span.trace.packet.push(packet);
                    // No need to flush - will happen when the span exits
                } else {
                    let process_name = process.name.as_deref();
                    let thread_name = thread.and_then(|t| t.name);
                    let trace = idl::Trace {
                        // Not in a span.
                        // Process and thread track *must* be present *before* the event
                        // Create the trace *after* the process and thread track give vec! a hint about the size.
                        packet: vec![
                            self.process_descriptor(pid, process_name),
                            self.thread_descriptor(pid, tid, thread_name.as_deref()),
                            packet,
                        ],
                    };
                    self.append(&trace)?;
                }
            }
        }

        Ok(())
    }

    fn process_descriptor(&self, id: u32, name: Option<&str>) -> idl::TracePacket {
        let mut packet = idl::TracePacket::default();
        let process = create_process_descriptor(id, name).into();
        let track_desc = create_track_descriptor(Some(self.track_uuid), name, process, None);
        packet.data = Some(idl::trace_packet::Data::TrackDescriptor(track_desc));
        packet
    }

    fn thread_descriptor(&self, tgid: u32, tid: u32, name: Option<&str>) -> idl::TracePacket {
        let mut packet = idl::TracePacket::default();
        let thread = create_thread_descriptor(tgid, tid).into();
        let track_desc = create_track_descriptor(Some(tid as u64), name, None, thread);
        packet.data = Some(idl::trace_packet::Data::TrackDescriptor(track_desc));
        packet
    }

    /// Append a trace packet to the writer. Serialized into proto and written to the writer.
    fn append(&mut self, packet: &idl::Trace) -> Result<(), Error> {
        let buf = packet.encode_to_vec();
        self.writer.0.write_all(&buf)?;
        self.writer.1 += buf.len() as u64;
        Ok(())
    }

    fn sequence_id(&self) -> idl::trace_packet::OptionalTrustedPacketSequenceId {
        idl::trace_packet::OptionalTrustedPacketSequenceId::TrustedPacketSequenceId(
            self.sequence_id,
        )
    }
}

fn create_process_descriptor(tgid: u32, name: Option<&str>) -> idl::ProcessDescriptor {
    perfetto_model::ProcessDescriptor {
        pid: Some(tgid as _),
        process_name: name.map(str::to_string),
        ..Default::default()
    }
}

fn create_thread_descriptor(tgid: u32, tid: u32) -> idl::ThreadDescriptor {
    perfetto_model::ThreadDescriptor {
        pid: Some(tgid as _),
        tid: Some(tid as _),
        ..Default::default()
    }
}

fn create_track_descriptor(
    uuid: Option<u64>,
    name: Option<&str>,
    process: Option<idl::ProcessDescriptor>,
    thread: Option<idl::ThreadDescriptor>,
) -> idl::TrackDescriptor {
    perfetto_model::TrackDescriptor {
        uuid,
        static_or_dynamic_name: name
            .map(|s| s.to_string())
            .map(idl::track_descriptor::StaticOrDynamicName::Name),
        process,
        thread,
        ..Default::default()
    }
}

fn create_event(
    track_uuid: u64,
    name: Option<&str>,
    location: Option<(&str, u32)>,
    debug_annotations: Option<DebugAnnotations>,
    r#type: Option<idl::track_event::Type>,
) -> idl::TrackEvent {
    let location = location.map(|(file, line)| {
        let source_location = perfetto_model::SourceLocation {
            file_name: Some(file.to_owned()),
            line_number: Some(line),
            ..Default::default()
        };
        idl::track_event::SourceLocationField::SourceLocation(source_location)
    });

    perfetto_model::TrackEvent {
        r#type: r#type.map(Into::into),
        track_uuid: Some(track_uuid),
        name_field: name.map(|name| idl::track_event::NameField::Name(name.to_string())),
        debug_annotations: debug_annotations.map(|d| d.annotations).unwrap_or_default(),
        source_location_field: location,
        ..Default::default()
    }
}

fn metadata_name(value: &Value) -> Option<&str> {
    value
        .as_object()
        .and_then(|o| o.get("metadata"))
        .and_then(|o| o.get("name"))
        .and_then(|s| s.as_str())
}

fn metadata_location(value: &Value) -> Option<(&str, u32)> {
    let file = value
        .as_object()
        .and_then(|o| o.get("metadata"))
        .and_then(|o| o.get("file"))
        .and_then(|s| s.as_str());
    let line = value
        .as_object()
        .and_then(|o| o.get("metadata"))
        .and_then(|o| o.get("line"))
        .and_then(|s| s.as_u64());
    match (file, line) {
        (Some(file), Some(line)) => Some((file, line as u32)),
        _ => None,
    }
}

#[derive(Default)]
struct DebugAnnotations {
    annotations: Vec<idl::DebugAnnotation>,
}

fn debug_annotations(value: &serde_json::Value) -> Option<DebugAnnotations> {
    let annotations = value
        .as_object()?
        .iter()
        .map(|(key, value)| debug_annotation(Some(key), value))
        .collect();

    Some(DebugAnnotations { annotations })
}

fn debug_annotation(name: Option<&str>, value: &Value) -> idl::DebugAnnotation {
    let name_field = name.map(|name| idl::debug_annotation::NameField::Name(name.to_string()));

    match value {
        serde_json::Value::Null => idl::DebugAnnotation {
            name_field,
            ..Default::default()
        },
        serde_json::Value::Bool(b) => idl::DebugAnnotation {
            name_field,
            value: Some(idl::debug_annotation::Value::BoolValue(*b)),
            ..Default::default()
        },
        serde_json::Value::Number(number) => idl::DebugAnnotation {
            name_field,
            value: number.as_i64().map(idl::debug_annotation::Value::IntValue),
            ..Default::default()
        },
        serde_json::Value::String(string) => idl::DebugAnnotation {
            name_field,
            value: Some(idl::debug_annotation::Value::StringValue(string.clone())),
            ..Default::default()
        },
        serde_json::Value::Array(vec) => idl::DebugAnnotation {
            name_field,
            array_values: vec
                .iter()
                .map(|value| debug_annotation(None, value))
                .collect(),
            ..Default::default()
        },
        serde_json::Value::Object(map) => idl::DebugAnnotation {
            name_field,
            dict_entries: map
                .iter()
                .map(|(key, value)| debug_annotation(Some(key), value))
                .collect(),
            ..Default::default()
        },
    }
}
