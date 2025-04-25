// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

#include "lane_assist.h"

#include "feo_cpp/feo_macros.h"
#include <cstdint>

LaneAssist::LaneAssist(const uint64_t activity_id) {
    this->activity_id = activity_id;
}

void LaneAssist::startup() {}

void LaneAssist::step() {}

void LaneAssist::shutdown() {}

// Create glue code for interface to Rust
MAKE_ACTIVITY(LaneAssist, lane_assist);

