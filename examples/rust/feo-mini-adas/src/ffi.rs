// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

use crate::activities::messages::{Scene, Steering};
use std::ffi::c_void;

#[link(name = "libactivities_cc")]
extern "C" {
    pub fn create_lane_assist(activity_id: usize) -> *mut c_void;

    pub fn startup_lane_assist(lane_assist_p: *mut c_void);

    pub fn step_lane_assist(
        lane_assist_p: *mut c_void,
        input_scene: &Scene,
        output_steering: &mut Steering,
    );

    pub fn shutdown_lane_assist(lane_assist_p: *mut c_void);

    pub fn free_lane_assist(lane_assist_p: *mut c_void);
}
