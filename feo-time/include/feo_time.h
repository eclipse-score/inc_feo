// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

#ifndef __FEO_TIME_H__
#define __FEO_TIME_H__

#ifdef __cplusplus
extern "C" {
#endif

#include <stdint.h>
#include <time.h>

/// Time in seconds and nanoseconds.
struct feo_timespec {
    time_t tv_sec;
    uint32_t tv_nsec;
};

// Set the clock speed factor
void feo_clock_speed(int factor);

// Get the current realtime
void feo_clock_gettime(struct feo_timespec* ts);

#ifdef __cplusplus
}  // extern "C"
#endif

#endif  // __FEO_TIME_H__
