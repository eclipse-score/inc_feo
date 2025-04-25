// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

#include "trajec_vis.h"

#include "feo_cpp/feo_macros.h"
#include <cstdint>

TrajectoryVisualizer::TrajectoryVisualizer(const uint64_t activity_id) {
    this->activity_id = activity_id;
}

void TrajectoryVisualizer::startup() {}

void TrajectoryVisualizer::step() {}

void TrajectoryVisualizer::shutdown() {}


// Create glue code for interface to Rust
MAKE_ACTIVITY(TrajectoryVisualizer, trajectory_visualizer);

