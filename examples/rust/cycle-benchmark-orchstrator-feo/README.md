# Cycle time benchmark 

A simple configurable benchmarking application to measure the FEO cycle time in various signalling and
single- or multi-agent setups.

## Running

Adapt the config file `config/cycle_bench.json` to reflect your desired setup. Then run one or more
instances of the application in different terminals:

```sh
cargo run --release --bin cycle_bench -- <AGENT_ID> [TARGET_CYCLE_TIME]
```

The first command line parameter specifies the agent ID of the process to be started. The second parameter defines
the the target FEO cycle time for the scheduler. It is only relevant for the primary agent and defaults to 5ms if
not given.

If the specified agent ID is equal to the primary ID defined in the config file, the above command will start the
primary agent. If it is equal to one of the secondary agent IDs (i.e. an agent ID in the config that is not
equal to the primary ID), it will start a secondary agent. Finally, if the agent ID matches one of the
recorder IDs from the config file, a recorder will be started.

