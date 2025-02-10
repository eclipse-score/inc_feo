// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

//! Messages
//!
//! This module contains the definition of messages
//! to be used within this example.

#[cfg(feature = "recording")]
use feo::{recording::registry::TypeRegistry, register_type, register_types};
#[cfg(feature = "recording")]
use postcard::experimental::max_size::MaxSize;
#[cfg(feature = "recording")]
use serde::{Deserialize, Serialize};

/// Camera image
///
/// A neural network could detect the number of people,
/// number of cars and the distance to the closest obstacle.
/// Given that we do not have a real neural network,
/// we already include information to be dummy inferred.
#[cfg_attr(feature = "recording", derive(Serialize, Deserialize, MaxSize))]
#[derive(Debug)]
#[repr(C)]
pub struct CameraImage {
    pub num_people: usize,
    pub num_cars: usize,
    pub distance_obstacle: f64,
}

/// Radar scan
///
/// With post-processing, we could detect the closest object
/// from a real radar scan. In this example,
/// the message type already carries the information to be dummy extracted.
#[cfg_attr(feature = "recording", derive(Serialize, Deserialize, MaxSize))]
#[derive(Debug)]
#[repr(C)]
pub struct RadarScan {
    pub distance_obstacle: f64,
    pub error_margin: f64,
}

/// Scene
///
/// The scene is the result of fusing the camera image and the radar scan
/// with a neural network. In our example, we just extract the information.
#[cfg_attr(feature = "recording", derive(Serialize, Deserialize, MaxSize))]
#[derive(Debug)]
#[repr(C)]
pub struct Scene {
    pub num_people: usize,
    pub num_cars: usize,
    pub distance_obstacle: f64,
    pub distance_left_lane: f64,
    pub distance_right_lane: f64,
}

/// Brake instruction
///
/// This is an instruction whether to engage the brakes and at which level.
#[cfg_attr(feature = "recording", derive(Serialize, Deserialize, MaxSize))]
#[derive(Debug)]
#[repr(C)]
pub struct BrakeInstruction {
    pub active: bool,
    pub level: f64,
}

/// Steering
///
/// This carries the angle of steering.
#[cfg_attr(feature = "recording", derive(Serialize, Deserialize, MaxSize))]
#[derive(Debug, Default)]
#[repr(C)]
pub struct Steering {
    pub angle: f64,
}

/// Return a type registry containing the types defined in this file
#[cfg(feature = "recording")]
pub fn type_registry() -> TypeRegistry {
    let mut registry = TypeRegistry::default();
    register_types!(
        registry,
        CameraImage,
        RadarScan,
        Scene,
        BrakeInstruction,
        Steering
    );
    registry
}
