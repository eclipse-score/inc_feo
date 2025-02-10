# mini-adas-recording

Example of a minimal ADAS activity set with data recording.

Run in four terminals:

```sh
cargo run --bin adas_recording_primary
```

```sh
cargo run --bin adas_recording_secondary_1
```

```sh
cargo run --bin adas_recording_secondary_2
```

```sh
cargo run --features recording --bin adas_recorder
```
