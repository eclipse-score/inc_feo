# feo-logger

The `feo-logger` is a logger in the terms of the `log` crate. It registers a global entity that acts as a sink for `feo_log::debug!` and friends calls.
The logs are then forwarded to a sink. Collection of records and serialization is done allocation free. Currently two sinks are implemented:

## console

Simple sink that logs to stdout. This maybe useful for debugging.

## logd

Connect to `logd` and forward each log record to it. See `logd` for details.