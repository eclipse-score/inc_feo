# score-feo

## Rust setup

You can build the Rust code both with `bazel` and with `cargo`.
The CI will run both builds and ensure neither one is broken.

### Bazel specific commands

Lint Rust code (clippy)

```sh
bazel build --config=lint-rust //...
```

## Bazel quick-start

The recommended way to run `bazel` is with [`bazelisk`][bazelisk].
On linux, this means downloading the binary from the releases page
and symlinking it to `bazel` somewhere on the `PATH`.

### Bazel examples

Build the whole workspace

```sh
bazel build //...
```

Test the whole workspace

```sh
bazel test //...
```

Query for targets

```sh
bazel query //...
```

Run example rust binary

```sh
bazel run //examples/rust/greeter:greeter_rust
```

[bazelisk]: https://github.com/bazelbuild/bazelisk

## Profiling

### CPU

[Samply](https://github.com/mstange/samply) is a convenient tool to profile `perf` based. The main goal is to simplify the usage of `perf` and provide a web interface to analyze the results without the need to perform manual steps.

Get your copy of *samply*:

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/mstange/samply/releases/download/samply-v0.12.0/samply-installer.sh | sh
```

Profile:

```sh
cargo build --example hello_tracing --profile profiling
samply record target/profiling/examples/hello_tracing
# ...
# <ctrl-c>
```
Samply will spawn a webserver on [https://127.0.0.1:3000](https://127.0.0.1:3000) by default. Open and enjoy the results.
The [Firefox Profiler](https://profiler.firefox.com) requires Firefox or Chrome (Safari is not supported).

### Memory

Easiest way to profile memory usage is to use
[bytehound](https://github.com/koute/bytehound). It is a tool that can be used
to profile memory usage of a binary or verify that a binary is allocation free.
It supports Linux only at the moment.

Install *bytehound* with the following commands:

```sh
wget https://github.com/koute/bytehound/releases/download/0.11.0/bytehound-x86_64-unknown-linux-gnu.tgz
tar xzf bytehound-x86_64-unknown-linux-gnu.tgz bytehound libbytehound.so 
mv bytehound libbytehound.so $HOME/.cargo/bin
```

Record something with bytehound:

```sh
LD_PRELOAD=$HOME/.cargo/bin/libbytehound.so target/debug/examples/hello_tracing
# ...
# <ctrl-c>
```

Done with recording.  Analyze the recording by 

```sh
bytehound server memory-profiling_*.dat
# [2025-01-16T10:15:18Z INFO  server_core] Trying to load "memory-profiling_hello_tracing_1737022463_1792106.dat"...
# [2025-01-16T10:15:18Z INFO  cli_core::loader] Loaded data in 0s 099
# [2025-01-16T10:15:18Z INFO  actix_server::builder] Starting 96 workers
# [2025-01-16T10:15:18Z INFO  actix_server::builder] Starting server on 127.0.0.1:8080
```

Click on the [link](http://127.0.0.1:8080) in the output to open the browser and
see the results. Setup ssh port forwarding if needed when working remote (`ssh -L 8080:localhost:8080 host`).
