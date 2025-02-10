// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

// Logging API

#ifndef __FEO_LOG_H__
#define __FEO_LOG_H__

// Log a message on the trace level
#define trace(tag, fmt, ...) feo::log::log(__FILE__, __LINE__, feo::log::Level::Trace, tag, fmt, ##__VA_ARGS__)

// Log a message on the debug level
#define debug(tag, fmt, ...) feo::log::log(__FILE__, __LINE__, feo::log::Level::Debug, tag, fmt, ##__VA_ARGS__)

// Log a message on the info level
#define info(tag, fmt, ...) feo::log::log(__FILE__, __LINE__, feo::log::Level::Info, tag, fmt, ##__VA_ARGS__)

// Log a message on the warn level
#define warn(tag, fmt, ...) feo::log::log(__FILE__, __LINE__, feo::log::Level::Warn, tag, fmt, ##__VA_ARGS__)

// Log a message on the error level
#define error(tag, fmt, ...) feo::log::log(__FILE__, __LINE__, feo::log::Level::Error, tag, fmt, ##__VA_ARGS__)

namespace feo {
namespace log {

// Log severity levels
enum Level { Error = 1, Warn = 2, Info = 3, Debug = 4, Trace = 5 };

// Log level filter
enum LevelFilter { OFF = 0, ERROR = 1, WARN = 2, INFO = 3, DEBUG = 4, TRACE = 5 };

// Log function
void log(const char* file, int line, Level level, const char* tag, const char* fmt, ...);

// Sets the global maximum log level.
// Generally, this should only be called by the active logging implementation.
// Note that Trace is the maximum level, because it provides the maximum amount of detail in the emitted logs.
void set_max_level(LevelFilter level);

// Returns the current maximum log level.
LevelFilter max_level();

}  // namespace log
}  // namespace feo

#endif  // __FEO_LOG_H__