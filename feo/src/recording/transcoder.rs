// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

//! Transcoders between com layer format and serialization for recording

use alloc::boxed::Box;
use alloc::string::String;
use core::ops::Deref as _;
use feo_com::interface::ActivityInput;
use serde::Serialize;

/// Transcode data of the given type from com layer representation to recording serialization
pub(crate) struct RecordingTranscoder<T: Serialize + 'static + core::fmt::Debug> {
    input: Box<dyn ActivityInput<T>>,
    topic: String,
    type_name: String,
}

impl<T: Serialize + postcard::experimental::max_size::MaxSize + core::fmt::Debug>
    RecordingTranscoder<T>
{
    /// Create a transcoder reading from the given com layer topic
    pub fn build(
        input_builder: impl Fn(&str) -> Box<dyn ActivityInput<T>> + Send,
        topic: String,
        type_name: String,
    ) -> Box<dyn ComRecTranscoder> {
        let input = input_builder(&topic);
        Box::new(RecordingTranscoder::<T> {
            input,
            topic,
            type_name,
        })
    }

    /// Read com layer data and serialize them for recording
    pub fn read_and_serialize<'a>(&self, buf: &'a mut [u8]) -> Option<&'a mut [u8]> {
        let input = self.input.read();
        if let Ok(value) = input {
            let value = value.deref();
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
    fn topic(&self) -> &str;

    // Get the type name of data this transcoder is transcoding
    fn type_name(&self) -> &str;
}

/// Implement the recording-and-serialization trait for all [`RecordingTranscoder`] types
impl<T: Serialize + postcard::experimental::max_size::MaxSize + core::fmt::Debug> ComRecTranscoder
    for RecordingTranscoder<T>
{
    fn buffer_size(&self) -> usize {
        T::POSTCARD_MAX_SIZE
    }
    fn read_transcode<'a>(&self, buf: &'a mut [u8]) -> Option<&'a mut [u8]> {
        self.read_and_serialize(buf)
    }

    fn topic(&self) -> &str {
        &self.topic
    }

    // Get the type name of data this transcoder is transcoding
    fn type_name(&self) -> &str {
        &self.type_name
    }
}

/// Builder trait for a [`ComRecTranscoder`] object
///
/// A builder is a function taking a com layer topic and creating a [`ComRecTranscoder`] object
/// for that topic
pub trait ComRecTranscoderBuilder: Fn(&'static str) -> Box<dyn ComRecTranscoder> {}

/// Implement the builder trait for any function matching the [`ComRecTranscoderBuilder`] builder trait.
///
/// In particular, this will apply to the [`build`] method of [`RecordingTranscoder`]
impl<T: Fn(&'static str) -> Box<dyn ComRecTranscoder>> ComRecTranscoderBuilder for T {}
