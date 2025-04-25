// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

#ifndef MINIADAS_TRAJEC_VIS_H_
#define MINIADAS_TRAJEC_VIS_H_

#include <cstdint>

class TrajectoryVisualizer {

  public:
    explicit TrajectoryVisualizer(uint64_t activity_id);

    void startup();
    void step();
    void shutdown();

  private:
    uint64_t activity_id;
};

#endif