# Mini ADAS Example

Example of a minimal dummy ADAS activity set.
A recorder can be added to the example and switched on in the primary via a CLI flag.

## Running

You need to run the following commands in separate terminals.
If you want to benchmark, you should also add the `--release` flag.

```sh
# Use 400ms cycle time
cargo run --bin adas_primary 400
```

```sh
cargo run --bin adas_secondary 1
```

```sh
cargo run --bin adas_secondary 2
```

If you want to include recording,
you need to pass the recorder's agent ID to the primary
and start the recorder.

```sh
# Use 400ms cycle time
# Wait for recorder with ID 900
cargo run --bin adas_primary 400 900
```

```sh
cargo run --bin adas_secondary 1
```

```sh
cargo run --bin adas_secondary 2
```

```sh
# Start recorder with ID 900
cargo run --features recording --bin adas_recorder 900
```

You may also use more than one recorder by specifying multiple recorder agent ids as a dot-separated
list to the primary and then start all the corresponding recorders.


```sh
# Use 400ms cycle time
# Wait for two recorders with IDs 900 and 901
cargo run --bin adas_primary 400 900.901
```

```sh
cargo run --bin adas_secondary 1
```

```sh
cargo run --bin adas_secondary 2
```

```sh
# Start recorder with ID 900
cargo run --features recording --bin adas_recorder 900
```

```sh
# Start recorder with ID 901
cargo run --features recording --bin adas_recorder 901
```


## Different signalling layer

The easiest way to switch the signalling layer is by changing the default feature in the `Cargo.toml`.
Then you can just use the commands from above.

Alternatively, you can pass `--no-default-features` and the right `--features` flag.

```sh
# Use 400ms cycle time
cargo run --no-default-features --features signalling_direct_tcp --bin adas_primary 400
```

```sh
cargo run --no-default-features --features signalling_direct_tcp --bin adas_secondary 1
```

```sh
cargo run --no-default-features --features signalling_direct_tcp --bin adas_secondary 2
```

Note that for mpsc-only signalling, there can be only a primary process without
any secondaries or recorders, because mpsc does not support inter-process signalling.