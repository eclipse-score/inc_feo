// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

#ifndef MINIADAS_LANE_ASIST_H_
#define MINIADAS_LANE_ASIST_H_

#include <cstdint>

class LaneAssist {

  public:
    explicit LaneAssist(uint64_t activity_id);

    void startup();
    void step();
    void shutdown();

  private:
    uint64_t activity_id;
};

#endif
