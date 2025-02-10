// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

#ifndef FEO_LOGGER_H

#include <log.h>

namespace feo {
namespace logger {

/// Initialize the logger.
/// Declare the minimum log level, whether to log to the console, and whether to log to the system log.
void init(feo::log::LevelFilter level_filter, bool console, bool logd);

}  // namespace logger
}  // namespace feo

#endif
