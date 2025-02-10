# feo-tracing

Applications use the `feo-tracing` provided API to instrument it's code to
generate traces. The traces are collected by a subscriber if
`feo-tracing::init()` is called. The subscriber tries to connect via a unix
socket to an instance of `feo-tracer` running on the same machine.
`feo-tracer` collects trace data from multiple applications and dumps into a
proto model that can be visualized using [perfetto.dev](https://ui.perfetto.dev).

Minimal example application code:
```
use feo_tracing::{event, Level};
fn main() {
    feo_tracing::init();
    event!(Level::DEBUG, "hello");
}

```

## How to run the example?

1. Start the `feo-tracer` binary. Do not stop the example.

```sh
cargo run --bin feo-tracer -- --out /tmp/feo.pftrace
```

2. Run the example application.

```sh
cargo run --example hello_tracing
```

3. Wait some seconds
4. Stop the `feo-tracer` binary by Ctrl+C
5. Open [perfetto.dev](https://ui.perfetto.dev) and upload `/tmp/feo.pftrace`.