// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

use crate::activity::ActivityId;
use crate::error::Error;
use crate::signalling::{
    AgentId, IntraProcReceiver, IntraProcSender, MioMultiSocketReceiver, MioMultiSocketSender,
    MioSocketReceiver, Receiver, Sender, Signal,
};
use crate::timestamp::{self, timestamp};
use crate::worker_pool::{WorkerId, WorkerPool};
use feo_log::{debug, error, info, trace, warn};
use feo_time::{Duration, Instant};
use mio::net::{TcpListener, TcpStream};
use mio::{Events, Interest, Poll, Token};
use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::thread;

pub struct PrimaryAgentConfig {
    /// The id of the agent
    pub agent_id: AgentId,

    /// The socket address on which to listen for connections from secondary agents
    pub bind_addr: SocketAddr,

    /// The target duration of a fixed execution order task chain cycle
    pub cycle_time: Duration,

    /// Overall map of task assignment to agents and workers
    pub agent_map: HashMap<AgentId, HashMap<WorkerId, Vec<ActivityId>>>,

    /// List of agent IDs of attached recorders
    pub recorders: Option<HashSet<AgentId>>,

    /// For each activity the list of activities it depends on
    pub activity_depends: HashMap<ActivityId, Vec<ActivityId>>,

    /// The optional worker pool run by the primary agent
    pub local_worker_pool: Option<WorkerPool>,

    /// Intra-process (ready) signal sender connected to the ready signal receiver
    pub intra_ready_sender: IntraProcSender<Signal>,

    /// Intra-process receiver of (ready) signals from all activities
    pub intra_ready_receiver: IntraProcReceiver<Signal>,
}

/// Implementation of the primary FEO agent
pub struct PrimaryAgent {
    scheduler: Scheduler,
}

impl PrimaryAgent {
    /// Create a new primary agent from the given configuration
    pub fn new(config: PrimaryAgentConfig) -> Self {
        let PrimaryAgentConfig {
            agent_id,
            bind_addr,
            cycle_time,
            agent_map,
            recorders,
            activity_depends,
            local_worker_pool,
            intra_ready_sender,
            intra_ready_receiver,
        } = config;

        let activity_connector = ActivityConnector::new(
            &agent_map,
            recorders.unwrap_or(HashSet::default()),
            agent_id,
            bind_addr,
            intra_ready_sender,
            intra_ready_receiver,
            local_worker_pool,
        );

        let scheduler = Scheduler::new(cycle_time, activity_depends, activity_connector);
        Self { scheduler }
    }

    pub fn run(&mut self) {
        // Initialize local time
        timestamp::initialize();

        // Connect to remote agents
        self.scheduler.connect_remotes();

        // synchronize timestamps by distribute system startup time
        self.scheduler.sync_remotes();

        // Run the FEO execution loop
        self.scheduler.run();
    }
}

/// Current state of an activity
struct ActivityState {
    /// Whether the activity has been triggered for an action
    triggered: bool,

    /// Whether the activity has finished its previously triggered operation
    ready: bool,
}

/// Global activity scheduler
///
/// The scheduler (aka 'FEO Executor') executes the FEO activities according to the defined order
struct Scheduler {
    /// Target duration of a task chain cycle
    cycle_time: Duration,

    /// For each activity: list of activities it depends on
    activity_depends: HashMap<ActivityId, Vec<ActivityId>>,

    /// Helper object connecting to activities in all connected agents
    activity_connector: ActivityConnector,

    /// Map keeping track of activity states
    activity_states: HashMap<ActivityId, ActivityState>,
}

impl Scheduler {
    fn new(
        feo_cycle_time: Duration,
        activity_depends: HashMap<ActivityId, Vec<ActivityId>>,
        activity_connector: ActivityConnector,
    ) -> Self {
        // Pre-allocate state map
        let activity_states: HashMap<ActivityId, ActivityState> = activity_depends
            .keys()
            .map(|k| {
                (
                    *k,
                    ActivityState {
                        triggered: false,
                        ready: false,
                    },
                )
            })
            .collect();

        Self {
            cycle_time: feo_cycle_time,
            activity_depends,
            activity_connector,
            activity_states,
        }
    }

    /// Connect to all expected secondary agents and recorders (i.e. all remote processes)
    pub fn connect_remotes(&mut self) {
        self.activity_connector.connect_remotes()
    }

    /// Synchronize all remote agents and recorders
    pub fn sync_remotes(&mut self) {
        self.activity_connector.sync_time();
        info!("Time synchronization of remote agents done");
    }

    /// Run the task lifecycle, i.e. startup, stepping, shutdown
    ///
    /// Shutdown is not implemented, as it is not yet defined in the architecture
    pub fn run(&mut self) {
        // Sort activity ids
        let mut activity_ids: Vec<_> = self.activity_states.keys().collect();
        activity_ids.sort();

        // Call startup on all activities sorted according to their ids
        // Note: Actual startup may occur in different order, depending on the assignment
        // of activities to worker threads. (A worker with greater id value may start up in
        // one thread before an activity with smaller id value in another thread.)
        for activity_id in activity_ids {
            self.activity_connector.startup_activity(activity_id)
        }

        // Wait until all activities have returned their ready signal
        while !self.is_all_ready() {
            self.wait_next_ready()
                .expect("failed while waiting for ready signal");
        }

        // Loop the FEO task chain
        loop {
            let task_chain_start = Instant::now();

            // Record start of task chain on registered recorders
            self.activity_connector.record_task_chain_start();

            // Clear ready and triggered signals
            self.activity_states.values_mut().for_each(|v| {
                v.ready = false;
                v.triggered = false;
            });

            debug!("Starting task chain");

            while !self.is_all_ready() {
                // Step all activities that have their dependencies met
                self.step_foreach_ready();
                // Wait until a new ready signal has been received
                self.wait_next_ready()
                    .expect("failed while waiting for ready signal");
            }

            // Record end of task chain on registered recorders => recorders will flush
            // => wait until all recorders have signalled to be ready
            trace!("Flushing recorders");
            let start_flush = Instant::now();
            self.activity_connector.record_task_chain_end();
            self.activity_connector.wait_recorders_ready();
            let flush_duration = start_flush.elapsed();
            trace!("Flushing recorders took {flush_duration:?}");

            let task_chain_duration = task_chain_start.elapsed();
            let time_left = self.cycle_time.saturating_sub(task_chain_duration);
            if time_left.is_zero() {
                error!(
                    "Finished task chain after {task_chain_duration:?}. Expected to be less than {:?}",
                    self.cycle_time
                );
            } else {
                debug!(
                    "Finished task chain after {task_chain_duration:?}. Sleeping for {time_left:?}"
                );
                thread::sleep(time_left);
            }
        }
    }

    /// Step each activity whose dependencies have signalled 'ready'
    fn step_foreach_ready(&mut self) {
        // Get data from activity_depends in self so that we can iterate over it
        // and at the same time modify another member of self
        for (act_id, dependencies) in self.activity_depends.iter() {
            // skip activity if already triggered
            if self.activity_states[act_id].triggered {
                continue;
            }

            // If dependencies are fulfilled
            let is_ready = self
                .activity_states
                .iter()
                .filter(|(id, _)| dependencies.contains(id))
                .all(|(_, state)| state.ready);
            if is_ready {
                self.activity_connector.step_activity(act_id);
                self.activity_states.get_mut(act_id).unwrap().triggered = true;
            }
        }
    }

    /// Wait for the next incoming ready signal
    fn wait_next_ready(&mut self) -> Result<(), Error> {
        // Wait for next intra-process ready signal from one of the workers
        let act_id = self.activity_connector.wait_next_ready()?;

        // Set corresponding ready flag
        self.activity_states.get_mut(&act_id).unwrap().ready = true;
        Ok(())
    }

    /// Check if all activities have signalled 'ready'
    fn is_all_ready(&self) -> bool {
        self.activity_states.values().all(|v| v.ready)
    }
}

struct IpcSignalReceiver {
    streams_ready: Option<HashMap<AgentId, TcpStream>>,
    intra_ready_sender: Option<IntraProcSender<Signal>>,
    _thread: Option<thread::JoinHandle<()>>,
}

impl IpcSignalReceiver {
    fn new(
        streams_ready: HashMap<AgentId, TcpStream>,
        intra_sender: IntraProcSender<Signal>,
    ) -> Self {
        IpcSignalReceiver {
            streams_ready: Some(streams_ready),
            intra_ready_sender: Some(intra_sender),
            _thread: None,
        }
    }

    fn thread_main(
        streams_ready: HashMap<AgentId, TcpStream>,
        mut intra_ready_send: impl Sender<Signal>,
    ) {
        let mut poll = Poll::new().unwrap();
        let mut events = Events::with_capacity(1024);
        let mut ipc_ready_receiver =
            MioMultiSocketReceiver::new(streams_ready, &mut poll, &mut events);
        ipc_ready_receiver.register().unwrap();

        loop {
            let (_, pdu) = ipc_ready_receiver.recv().unwrap();
            let signal = Signal::try_from(&pdu).unwrap();
            intra_ready_send.send(signal).unwrap();
        }
    }

    fn run(&mut self) {
        assert!(self._thread.is_none(), "thread is already running");

        // Move member variables out of self
        let streams_ready = self.streams_ready.take().expect("missing ready streams");
        let intra_ready_sender = self
            .intra_ready_sender
            .take()
            .expect("missing intra-process ready sender");

        // Start ready signal receiver thread
        self._thread = Some(thread::spawn(move || {
            IpcSignalReceiver::thread_main(streams_ready, intra_ready_sender)
        }));
    }
}

/// Handle signalling from and to all activities for the primary agent
struct ActivityConnector {
    /// ID of the primary agent
    local_agent_id: AgentId,

    /// Socket address on which to wait for connecting remote processing
    local_addr: SocketAddr,

    /// Map providing the IDs of agent and worker executing a given activity
    activity_map: HashMap<ActivityId, (AgentId, WorkerId)>,

    /// Set of connected recorders (possibly empty)
    recorders: HashSet<AgentId>,

    /// Map of recorders' ready states
    recorders_ready: HashMap<AgentId, bool>,

    /// List of all expected secondary agents
    secondary_agents: Vec<AgentId>,

    /// Sender to be used by the IPC receiver thread to transmit signals to this connector
    intra_ready_sender: IntraProcSender<Signal>,

    /// Receiver to be used by this connector to obtain signals from the IPC receiver thread
    intra_ready_receiver: IntraProcReceiver<Signal>,

    /// Reference to the local worker pool
    local_workpool: Option<WorkerPool>,

    /// Sender connecting to remote agents (secondaries and recorders)
    ipc_sender: Option<MioMultiSocketSender>,

    /// Helper for handling signals from the secondary agents
    ipc_receiver: Option<IpcSignalReceiver>,
}

impl ActivityConnector {
    pub fn new(
        agent_map: &HashMap<AgentId, HashMap<WorkerId, Vec<ActivityId>>>,
        recorders: HashSet<AgentId>,
        local_agent_id: AgentId,
        local_socket_addr: SocketAddr,
        intra_ready_sender: IntraProcSender<Signal>,
        intra_ready_receiver: IntraProcReceiver<Signal>,
        local_workpool: Option<WorkerPool>,
    ) -> Self {
        // Create map from ActivityId to corresponding AgentId and WorkerId
        let mut activity_map: HashMap<ActivityId, (AgentId, WorkerId)> = Default::default();
        for (agent_id, workers) in agent_map {
            for (worker_id, activity_group) in workers {
                for act_id in activity_group {
                    let previous = activity_map.insert(*act_id, (*agent_id, *worker_id));
                    assert!(
                        previous.is_none(),
                        "Duplicate activity {act_id} in assignment list"
                    )
                }
            }
        }

        // Collect IDs of secondary agents
        let secondary_agents: Vec<AgentId> = agent_map
            .keys()
            .copied()
            .filter(|x| *x != local_agent_id)
            .collect();

        // Pre-allocate recorder ready state map
        let recorders_ready: HashMap<AgentId, bool> =
            recorders.iter().map(|id| (*id, false)).collect();

        Self {
            local_agent_id,
            local_addr: local_socket_addr,
            activity_map,
            recorders,
            recorders_ready,
            secondary_agents,
            intra_ready_sender,
            intra_ready_receiver,
            local_workpool,
            ipc_sender: None,
            ipc_receiver: None,
        }
    }

    /// Wait for connection from expected secondary agents and recorders
    pub fn connect_remotes(&mut self) {
        let mut listener = mio::net::TcpListener::bind(self.local_addr)
            .unwrap_or_else(|e| panic!("failed to bind local socket: {e:?}"));
        let mut listen_events = Events::with_capacity(1024);
        let mut listen_poll =
            Poll::new().unwrap_or_else(|e| panic!("failed to create poll instance: {e:?}"));

        let mut connection_events = Events::with_capacity(1024);
        let mut connection_poll =
            Poll::new().unwrap_or_else(|e| panic!("failed to create poll instance: {e:?}"));

        listen_poll
            .registry()
            .register(&mut listener, Token(0), Interest::READABLE)
            .unwrap_or_else(|e| panic!("failed to register listener for polling: {e:?}"));

        let mut streams_trigger: HashMap<AgentId, TcpStream> = Default::default();
        let mut streams_ready: HashMap<AgentId, TcpStream> = Default::default();
        loop {
            let has_all_agent_trigger_streams = self
                .secondary_agents
                .iter()
                .all(|x| streams_trigger.contains_key(x));
            let has_all_recording_streams = self
                .recorders
                .iter()
                .all(|x| streams_trigger.contains_key(x));
            let has_all_agent_ready_streams = self
                .secondary_agents
                .iter()
                .all(|x| streams_ready.contains_key(x));
            let has_all_recording_ready_streams =
                self.recorders.iter().all(|x| streams_ready.contains_key(x));
            let has_all_conns = has_all_agent_trigger_streams
                && has_all_agent_ready_streams
                && has_all_recording_streams
                && has_all_recording_ready_streams;
            if has_all_conns {
                break;
            }

            // Wait for the next incoming connection with hello message and handle it,
            // i.e. determine the type of message and put the stream into the corresponding collection
            self.wait_and_handle_hello(
                &mut listen_poll,
                &mut listen_events,
                &listener,
                &mut connection_poll,
                &mut connection_events,
                &mut streams_trigger,
                &mut streams_ready,
            )
        }

        // Start ready signal handler
        self.ipc_receiver = Some(IpcSignalReceiver::new(
            streams_ready,
            self.intra_ready_sender.clone(),
        ));
        self.ipc_receiver.as_mut().unwrap().run();

        // Create sender to remote agents (secondaries and recorders)
        let streams_send: HashMap<AgentId, TcpStream> = streams_trigger.into_iter().collect();
        self.ipc_sender = Some(MioMultiSocketSender::new(streams_send));
    }

    /// Helper method: Wait for the next hello message from another agent
    #[allow(clippy::too_many_arguments)]
    fn wait_and_handle_hello(
        &mut self,
        listen_poll: &mut Poll,
        listen_events: &mut Events,
        listener: &TcpListener,
        connection_poll: &mut Poll,
        connection_events: &mut Events,
        streams_trigger: &mut HashMap<AgentId, TcpStream>,
        streams_ready: &mut HashMap<AgentId, TcpStream>,
    ) {
        listen_poll
            .poll(listen_events, None)
            .unwrap_or_else(|e| panic!("polling failed: {e:?}"));

        for event in listen_events.iter() {
            if event.token() == Token(0) {
                debug!("Received listener event");
                let (mut stream, addr) = listener
                    .accept()
                    .unwrap_or_else(|e| panic!("listener accept failed: {e:?}"));
                stream
                    .set_nodelay(true)
                    .unwrap_or_else(|e| panic!("setting nodelay for stream failed: {e:?}"));

                info!("Incoming connection from {addr}");
                let mut conn =
                    MioSocketReceiver::new(&mut stream, connection_poll, connection_events);
                conn.register(0)
                    .unwrap_or_else(|e| panic!("registering connection failed {e:?}"));
                let pdu = conn
                    .recv()
                    .unwrap_or_else(|e| panic!("reception of pdu failed {e:?}"));
                drop(conn);

                let signal = Signal::try_from(&pdu);

                // If a valid signal has been received, check if and which hello message it is,
                // then move the stream into the corresponding collection or drop it
                if let Ok(signal) = signal {
                    self.handle_hello(signal, stream, streams_trigger, streams_ready)
                } else {
                    warn!("Dropping stream with invalid signal");
                }
            }
        }
    }

    /// Handle the given signal received on the given stream as a hello message from an agent  
    fn handle_hello(
        &mut self,
        signal: Signal,
        stream: TcpStream,
        streams_trigger: &mut HashMap<AgentId, TcpStream>,
        streams_ready: &mut HashMap<AgentId, TcpStream>,
    ) {
        if let Signal::HelloTrigger(id) = signal {
            debug!("Received 'hello_trigger' from {id}");
            if self.secondary_agents.contains(&id) || self.recorders.contains(&id) {
                if let Entry::Vacant(e) = streams_trigger.entry(id) {
                    e.insert(stream);
                    info!("Received 'hello_trigger' from expected id {id}");
                } else {
                    warn!("Ignoring new 'hello_trigger' from already encountered id {id}")
                }
            } else {
                warn!("Ignoring 'hello_trigger' from unexpected id {id}")
            }
        } else if let Signal::HelloReady(id) = signal {
            debug!("Received 'hello_ready' from {id}");
            if self.secondary_agents.contains(&id) || self.recorders.contains(&id) {
                if let Entry::Vacant(e) = streams_ready.entry(id) {
                    e.insert(stream);
                    info!("Received 'hello_ready' from expected id {id}");
                } else {
                    warn!("Ignoring new 'hello_ready' from already encountered id {id}")
                }
            } else {
                warn!("Ignoring 'hello_ready' from unexpected id {id}")
            }
        } else {
            warn!("Dropping stream with signal {signal}");
        }
    }

    pub fn sync_time(&mut self) {
        let ipc_sender = self
            .ipc_sender
            .as_mut()
            .expect("activity connector not connected");

        // Send startup time to all secondary agents
        let signal = Signal::StartupSync(timestamp::sync_info());
        for agent_id in self.secondary_agents.iter() {
            ipc_sender.send((*agent_id, signal)).unwrap_or_else(|e| {
                panic!("failed to send signal {signal} to agent {agent_id}: {e:?}")
            });
        }

        // Send startup time to all recoders
        let signal = Signal::StartupSync(timestamp::sync_info());
        for agent_id in self.recorders.iter() {
            ipc_sender.send((*agent_id, signal)).unwrap_or_else(|e| {
                panic!("failed to send signal {signal} to agent {agent_id}: {e:?}")
            });
        }
    }

    /// Wait until the next Ready signal has been received and return the wrapped activity id
    pub fn wait_next_ready(&mut self) -> Result<ActivityId, Error> {
        // get the sender for distributing signals to the recorders
        let ipc_sender = self
            .ipc_sender
            .as_mut()
            .expect("activity connector not connected");

        // Wait for next intra-process ready signal from one of the workers
        // and return the corresponding activity ID
        loop {
            let signal: Signal = self.intra_ready_receiver.recv()?;
            if let Signal::Ready((id, _)) = signal {
                // Forward the signal to the recorders
                Self::record_signal(signal, &self.recorders, ipc_sender);
                return Ok(id);
            }
            error!("Received unexpected signal {signal:?} while waiting for ready signal");
        }
    }

    /// Wait until all the connected recorders have signalled ready
    pub fn wait_recorders_ready(&mut self) {
        // If there are no connected recorders, return immediately
        if self.recorders.is_empty() {
            return;
        }

        // Clear all ready flags
        self.recorders_ready.values_mut().for_each(|v| {
            *v = false;
        });

        // Loop until all recorders have signalled RecorderReady
        loop {
            // Wait for the next signal (from any agent, but only recorders are expected to send)
            let signal = match self.intra_ready_receiver.recv() {
                Ok(signal) => signal,
                Err(_) => {
                    error!("failed to receive signal, skipping recorder flush");
                    return;
                }
            };
            if let Signal::RecorderReady((id, _)) = signal {
                if self.recorders.contains(&id) {
                    // signal received, set ready entry of the corresponding recorder
                    let entry = self.recorders_ready.get_mut(&id).unwrap();
                    *entry = true;

                    // return, if all flags are set
                    if self.recorders_ready.values().all(|v| *v) {
                        return;
                    }
                } else {
                    error!("Received unexpected id {id} in recorder ready signal");
                }
            } else {
                error!(
                    "Received unexpected signal {signal} while waiting for recorder ready signal"
                );
            }
        }
    }

    /// Send the given signal to the corresponding activity.
    ///
    /// The activity may be on a remote process or in the local worker pool
    fn trigger_activity(&mut self, signal: Signal) {
        let activity_id = signal.activity_id().unwrap_or_else(|| {
            panic!("an activity cannot be triggered by the given signal {signal}")
        });
        let (agent_id, worker_id) = self
            .activity_map
            .get(&activity_id)
            .unwrap_or_else(|| panic!("missing agent entry for target activity {activity_id}"));

        trace!("Sending {signal} to worker {worker_id} at agent {agent_id}");

        // Get the sender for distributing signals to remote processes
        let ipc_sender = self
            .ipc_sender
            .as_mut()
            .expect("activity connector not connected");

        // If agent ID indicates a local agent, trigger local worker pool;
        // otherwise determine connection to remote agent and send signal command via IPC
        if *agent_id == self.local_agent_id {
            let worker_pool = self
                .local_workpool
                .as_mut()
                .expect("local worker pool is missing");
            worker_pool.trigger(signal);
        } else {
            ipc_sender.send((*agent_id, signal)).unwrap_or_else(|e| {
                panic!("failed to send signal {signal} to agent {agent_id}: {e:?}")
            });
        }

        // Send signal to the recorders
        Self::record_signal(signal, &self.recorders, ipc_sender);
    }

    /// Send step signal to the given activity
    pub fn step_activity(&mut self, id: &ActivityId) {
        debug!("Triggering step for activity {}", id);
        self.trigger_activity(Signal::Step((*id, timestamp())));
    }

    /// Send startup signal to the given activity
    pub fn startup_activity(&mut self, id: &ActivityId) {
        debug!("Triggering Startup for activity {}", id);
        self.trigger_activity(Signal::Startup((*id, timestamp())));
    }

    /// Send shutdown signal to the given activity
    #[allow(dead_code)]
    pub fn shutdown_activity(&mut self, id: &ActivityId) {
        // TODO: System Shutdown not yet specified => this method never gets called
        debug!("Triggering Shutdown for activity {}", id);
        self.trigger_activity(Signal::Shutdown((*id, timestamp())));
    }

    pub fn record_task_chain_start(&mut self) {
        trace!("Recording task chain start");
        // get the sender for distributing signals to the recorders
        let ipc_sender = self
            .ipc_sender
            .as_mut()
            .expect("activity connector not connected");
        let signal = Signal::TaskChainStart(timestamp());
        Self::record_signal(signal, &self.recorders, ipc_sender);
    }

    pub fn record_task_chain_end(&mut self) {
        trace!("Recording task chain end");
        // get the sender for distributing signals to the recorders
        let ipc_sender = self
            .ipc_sender
            .as_mut()
            .expect("activity connector not connected");
        let signal = Signal::TaskChainEnd(timestamp());
        Self::record_signal(signal, &self.recorders, ipc_sender);
    }

    /// Transmit the given signal for recording to the given recorders
    fn record_signal<'s, R>(signal: Signal, recorders: R, sender: &mut MioMultiSocketSender)
    where
        R: IntoIterator<Item = &'s AgentId>,
    {
        for agent_id in recorders.into_iter() {
            trace!("Sending {signal} to recorder {agent_id}");
            sender.send((*agent_id, signal)).unwrap_or_else(|e| {
                // if sending fails, signal an error and disconnect the recorder
                error!("Failed to send signal {signal} to recorder {agent_id}: {e:?}. Disconnecting recorder")
            });
        }
    }
}

pub fn run(mut agent: PrimaryAgent) {
    agent.run();
}
