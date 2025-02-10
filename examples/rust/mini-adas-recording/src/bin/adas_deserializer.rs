// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

use feo::recording::recorder::{DataDescriptionRecord, Record};
use feo_log::info;
use mini_adas_recording::activities::messages;
use serde::Deserialize;
use std::io::Read;

fn main() {
    feo_logger::init(feo_log::LevelFilter::Trace, true, false);

    let mut serialized_data = Vec::new();
    std::fs::File::open("rec.bin")
        .expect("failed to open recording")
        // reading to end for now, just for this simple tool
        .read_to_end(&mut serialized_data)
        .expect("failed to read recording");

    info!("Read file with {} bytes", serialized_data.len());
    let mut remaining_bytes = serialized_data.as_slice();
    while !remaining_bytes.is_empty() {
        let (record, remaining) =
            postcard::take_from_bytes(remaining_bytes).expect("deserializing failed");
        remaining_bytes = remaining;

        println!("{record:#?}");
        if let Record::DataDescription(data_record) = record {
            if let Some((image, remaining)) =
                try_deserialization_as_a::<messages::CameraImage>(data_record, remaining_bytes)
            {
                remaining_bytes = remaining;
                println!("{:#?}", image);
            } else if let Some((radar, remaining)) =
                try_deserialization_as_a::<messages::RadarScan>(data_record, remaining_bytes)
            {
                remaining_bytes = remaining;
                println!("{:#?}", radar);
            } else if let Some((scene, remaining)) =
                try_deserialization_as_a::<messages::Scene>(data_record, remaining_bytes)
            {
                remaining_bytes = remaining;
                println!("{:#?}", scene);
            } else if let Some((brake, remaining)) =
                try_deserialization_as_a::<messages::BrakeInstruction>(data_record, remaining_bytes)
            {
                remaining_bytes = remaining;
                println!("{:#?}", brake);
            } else if let Some((steering, remaining)) =
                try_deserialization_as_a::<messages::Steering>(data_record, remaining_bytes)
            {
                remaining_bytes = remaining;
                println!("{:#?}", steering);
            } else {
                // skip data record
                info!("Skipping deserialization of {}", data_record.type_name);
                remaining_bytes = &remaining_bytes[data_record.data_size..];
            }
        }
    }
}

fn try_deserialization_as_a<'a, T: Deserialize<'a>>(
    header: DataDescriptionRecord,
    bytes: &'a [u8],
) -> Option<(T, &'a [u8])> {
    if header.type_name == std::any::type_name::<T>() {
        Some(postcard::take_from_bytes(bytes).expect("failed to deserialize CameraImage"))
    } else {
        None
    }
}
