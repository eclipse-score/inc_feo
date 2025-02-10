// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

//! Transcoders between com layer format and serialization for recording

use crate::com::ActivityInput;
use serde::Serialize;

/// Transcode data of the given type from com layer representation to recording serialization
pub(crate) struct RecordingTranscoder<T: Serialize + 'static + std::fmt::Debug> {
    input: ActivityInput<T>,
    topic: &'static str,
    type_name: &'static str,
}

impl<T: Serialize + postcard::experimental::max_size::MaxSize + std::fmt::Debug>
    RecordingTranscoder<T>
{
    /// Create a transcoder reading from the given com layer topic
    pub fn build(topic: &'static str, type_name: &'static str) -> Box<dyn ComRecTranscoder> {
        Box::new(RecordingTranscoder::<T> {
            input: ActivityInput::get(topic),
            topic,
            type_name,
        })
    }

    /// Read com layer data and serialize them for recording
    pub fn read_and_serialize<'a>(&self, buf: &'a mut [u8]) -> Option<&'a mut [u8]> {
        let input = self.input.read();
        if let Some(input) = input {
            let value = input.get();
            feo_log::info!("Serializing {:?}", value);
            let written = postcard::to_slice(value, buf).expect("serialization failed");
            return Some(written);
        }
        None
    }
}

/// Trait implementing reading and transcoding of com data for recording
pub trait ComRecTranscoder {
    /// Read com layer data and serialize them for recording
    fn read_transcode<'a>(&self, buf: &'a mut [u8]) -> Option<&'a mut [u8]>;

    /// Maximum buffer size required for serialization
    fn buffer_size(&self) -> usize;

    // Get the topic to which this transcoder is connected
    fn topic(&self) -> &'static str;

    // Get the type name of data this transcoder is transcoding
    fn type_name(&self) -> &'static str;
}

/// Implement the recording-and-serialization trait for all [`RecordingTranscoder`] types
impl<T: Serialize + postcard::experimental::max_size::MaxSize + std::fmt::Debug> ComRecTranscoder
    for RecordingTranscoder<T>
{
    fn buffer_size(&self) -> usize {
        T::POSTCARD_MAX_SIZE
    }
    fn read_transcode<'a>(&self, buf: &'a mut [u8]) -> Option<&'a mut [u8]> {
        self.read_and_serialize(buf)
    }

    fn topic(&self) -> &'static str {
        self.topic
    }

    // Get the type name of data this transcoder is transcoding
    fn type_name(&self) -> &'static str {
        self.type_name
    }
}

/// Builder trait for a [`ComRecTranscoder`] object
///
/// A builder is a function taking a com layer topic and creating a [`ComRecTranscoder`] object
/// for that topic
pub trait ComRecTranscoderBuilder: Fn(&'static str) -> Box<dyn ComRecTranscoder> + Send {}

/// Implement the builder trait for any function matching the [`ComRecTranscoderBuilder`] builder trait.
///
/// In particular, this will apply to the [`build`] method of [`RecordingTranscoder`]
impl<T: Fn(&'static str) -> Box<dyn ComRecTranscoder> + Send> ComRecTranscoderBuilder for T {}
