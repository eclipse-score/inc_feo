// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

#include <cstdlib>
#include <ctime>
#include <thread>

#include <log.h>
#include <logger.h>

using namespace feo::log;

// Tag for the log messages
static const char* TAG = "hello::main";

// Sleep for a random amount of time
void sleep(int max_ms);

void log() {}

// Log messages in a loop, demonstrating the different log levels
void do_it(int i) {
    for (;;) {
        trace(TAG, "Hello %d!", i);
        sleep(500);
        debug(TAG, "Hello %d!", i);
        sleep(500);
        info(TAG, "Hello %d!", i);
        sleep(500);
        warn(TAG, "Hello %d!", i);
        sleep(500);
        error(TAG, "Hello %d!", i);
        sleep(2000);
    }
}

int main(int argc, char* argv[]) {
    // Initialize the logger with the maximum log level set to TRACE
    // Log to the console *and* the system log
    feo::logger::init(TRACE, true, true);

    // Do a trace log
    trace(TAG, "Hi - very spammy trace log. You won't see that again");

    // Adjust the maximum log level
    feo::log::set_max_level(LevelFilter::DEBUG);

    // Spawn threads that randomly log messages
    auto a = std::thread(do_it, 1);
    auto b = std::thread(do_it, 2);

    // Wait for the threads to finish
    a.join();
    b.join();

    return 0;
}

void sleep(int max_ms) {
    std::this_thread::sleep_for(std::chrono::milliseconds(std::rand() % max_ms));
}
