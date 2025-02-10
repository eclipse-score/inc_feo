// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

#include <cstdint>

typedef struct Scene {
    uint64_t num_people;
    uint64_t num_cars;
    double distance_obstacle;
    double distance_left_lane;
    double distance_right_lane;
} Scene;

typedef struct Steering {
    double angle;
} Steering;

class LaneAssist {
    int activity_id;

  public:
    LaneAssist(uint64_t activity_id);

    void startup();
    void step(const Scene& input_scene, Steering& output_steering);
    void shutdown();
};
