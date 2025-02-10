// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

#include <feo_time.h>
#include <gtest/gtest.h>

namespace time_test {
TEST(time, timespec) {
    struct feo_timespec ts;
    feo_clock_gettime(&ts);
    EXPECT_GT(ts.tv_sec, 0);
    EXPECT_GT(ts.tv_nsec, 0);
}
}  // namespace time_test
