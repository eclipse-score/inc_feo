// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

// Log implementation.

#include <log.h>
#include <cstdarg>
#include <cstdio>

// See rust `feo_log::MAX_RECORD_SIZE`.
static const size_t MAX_RECORD_SIZE = 8 * 1024;

extern "C" void __log(const char* file, int line, int level, const char* tag, const char* message);
extern "C" void __set_max_level(int level);
extern "C" int __max_level();

namespace feo {
namespace log {

/// Forward a log message. Flatten the format string. Other parameters are passed through.
void log(const char* file, int line, Level level, const char* tag, const char* fmt, ...) {
    char message[MAX_RECORD_SIZE];
    va_list vl;
    va_start(vl, fmt);
    vsnprintf(message, sizeof(message), fmt, vl);
    va_end(vl);

    __log(file, line, level, tag, message);
}

/// Set the maximum log level.
void set_max_level(LevelFilter level) {
    __set_max_level((int)level);
}

/// Return the maximum log level.
LevelFilter max_level() {
    return (LevelFilter)__max_level();
}

}  // namespace log
}  // namespace feo