// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

#include "include/activities.h"

LaneAssist::LaneAssist(uint64_t activity_id) {
    activity_id = activity_id;
}

void LaneAssist::startup() {}

void LaneAssist::step(const Scene& input_scene, Steering& output_steering) {
    // Calculate angle as difference of the distances for this example.
    double diff = input_scene.distance_left_lane - input_scene.distance_right_lane;
    output_steering.angle = diff;
}

void LaneAssist::shutdown() {}

extern "C" {
void* create_lane_assist(uint64_t activity_id) {
    LaneAssist* lane_assist = new LaneAssist(activity_id);
    return (void*)lane_assist;
}

void startup_lane_assist(void* lane_assist_p) {
    LaneAssist* lane_assist = (LaneAssist*)lane_assist_p;
    lane_assist->startup();
}

void step_lane_assist(void* lane_assist_p, const Scene& input_scene, Steering& output_steering) {
    LaneAssist* lane_assist = (LaneAssist*)lane_assist_p;
    lane_assist->step(input_scene, output_steering);
}

void shutdown_lane_assist(void* lane_assist_p) {
    LaneAssist* lane_assist = (LaneAssist*)lane_assist_p;
    lane_assist->shutdown();
}

void free_lane_assist(void* lane_assist_p) {
    LaneAssist* lane_assist = (LaneAssist*)lane_assist_p;
    delete lane_assist;
}
}
