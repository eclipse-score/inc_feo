// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

use feo::cpp_activity;

// Create glue code for C++ activities
// Note: Currently this only works for activities having no arguments to their C++ step function
//       (as is to be expected once the com layer is interoperable between Rust and C++)
cpp_activity!(lane_assist, "cpp_activities");
cpp_activity!(trajectory_visualizer, "cpp_activities");
