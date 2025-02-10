// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

#include <logger.h>

extern "C" void __init(int level_filter, bool console, bool logd);

namespace feo {
namespace logger {

void init(feo::log::LevelFilter level_filter, bool console, bool logd) {
    __init(level_filter, console, logd);
}

}  // namespace logger
}  // namespace feo