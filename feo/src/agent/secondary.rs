// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

use crate::activity::ActivityId;
use crate::error::Error;
use crate::signalling::inter_proc_socket::FdExt;
use crate::signalling::{
    AgentId, IntraProcReceiver, MioSocketReceiver, MioSocketSender, Receiver, Sender, Signal,
};
use crate::timestamp::{self, timestamp, SyncInfo};
use crate::worker_pool::{WorkerPool, WorkerPoolListener, WorkerPoolTrigger};
use feo_log::{debug, error, info};
use mio::net::TcpStream;
use mio::{Events, Poll};
use std::net::SocketAddr;
use std::thread;
use std::time::Duration;

pub struct SecondaryAgent {
    wp_listener: WorkerPoolListener,
    primary_connector: PrimaryConnector,
}

impl SecondaryAgent {
    pub fn new(
        agent_id: AgentId,
        remote_socket_addr: SocketAddr,
        worker_pool: WorkerPool,
        intra_ready_receiver: IntraProcReceiver<Signal>,
    ) -> Self {
        let wp_listener = worker_pool.listener(intra_ready_receiver);
        let (_, wp_trigger) = worker_pool.split();

        // create connector to primary agent
        let primary_connector = PrimaryConnector::new(agent_id, remote_socket_addr, wp_trigger);

        Self {
            wp_listener,
            primary_connector,
        }
    }

    fn run(&mut self) {
        self.connect_primary();

        loop {
            self.wp_listener.clear_ready();
            self.wp_listener.wait_next_ready();

            let ready_ids = self
                .wp_listener
                .ready_iter()
                .filter_map(|(id, ready)| ready.then_some(id));
            for id in ready_ids {
                if let Err(e) = self.primary_connector.send_ready(id) {
                    error!("Failed to transmit ready signal for activity ID {id}: {e}");
                }
            }
        }
    }

    fn connect_primary(&mut self) {
        self.primary_connector.connect_primary()
    }
}

struct IpcSignalReceiver {
    trigger_stream: Option<TcpStream>,
    workpool_trigger: Option<WorkerPoolTrigger>,
    _thread: Option<thread::JoinHandle<()>>,
}

impl IpcSignalReceiver {
    fn new(trigger_stream: TcpStream, wp_trigger: WorkerPoolTrigger) -> Self {
        IpcSignalReceiver {
            trigger_stream: Some(trigger_stream),
            workpool_trigger: Some(wp_trigger),
            _thread: None,
        }
    }

    /// Wait for and receive synchronization event from primary agent
    fn receive_sync(&mut self) -> SyncInfo {
        // Get trigger stream
        let trigger_stream = self
            .trigger_stream
            .as_mut()
            .expect("cannot synchronize: stream not yet or not anymore available");

        // Register stream with Poll
        let mut poll = Poll::new().unwrap();
        let mut events = Events::with_capacity(1024);
        let mut receiver = MioSocketReceiver::new(trigger_stream, &mut poll, &mut events);
        receiver.register(0).unwrap();

        // Wait until signal received
        debug!("Waiting for startup synchronization pdu");
        let signal: Signal = receiver
            .recv()
            .expect("failed to receive")
            .try_into()
            .expect("failed to decode signal pdu");
        debug!("Received signal {signal}");

        // Extract synchronization info or panic, if signal is incorrect
        let sync_info = match signal {
            Signal::StartupSync(info) => info,
            _ => panic!("received unexpected signal {signal}"),
        };

        // Deregister receiver from Poll
        receiver
            .deregister()
            .expect("failed to deregister receiver");

        // Return result
        sync_info
    }

    /// Thread main function waiting for and forwarding trigger signals from the primary process
    fn thread_main(trigger_stream: &mut TcpStream, workpool_trigger: &mut WorkerPoolTrigger) {
        let mut poll = Poll::new().unwrap();
        let mut events = Events::with_capacity(1024);
        let mut ipc_trigger_receiver =
            MioSocketReceiver::new(trigger_stream, &mut poll, &mut events);
        ipc_trigger_receiver.register(0).unwrap();
        loop {
            debug!("Waiting for trigger pdu");
            let signal: Signal = ipc_trigger_receiver
                .recv()
                .expect("failed to receive")
                .try_into()
                .expect("failed to decode signal pdu");
            debug!("Received signal {signal}");
            workpool_trigger.trigger(signal); // Forward the received signal to the worker pool
        }
    }

    /// Start the signal forwarding thread
    fn run(&mut self) {
        assert!(self._thread.is_none(), "thread is already running");

        // start ready signal receiver thread
        let mut trigger_stream = self.trigger_stream.take().unwrap();
        let mut workpool_trigger = self.workpool_trigger.take().unwrap();
        self._thread = Some(thread::spawn(move || {
            IpcSignalReceiver::thread_main(&mut trigger_stream, &mut workpool_trigger)
        }));
    }
}

/// Handle signalling from and to the primary agent
struct PrimaryConnector {
    // ID of the secondary agent
    local_agent_id: AgentId,

    // Socket address of the primary process
    remote_addr: SocketAddr,

    // Trigger interface to the local worker pool
    workpool_trigger: Option<WorkerPoolTrigger>,

    // Helper for handling signals from the primary agent
    ipc_receiver: Option<IpcSignalReceiver>,

    // IPC sender to the primary agent
    ipc_sender: Option<MioSocketSender<TcpStream>>,
}

impl PrimaryConnector {
    pub fn new(
        local_agent_id: AgentId,
        remote_socket_addr: SocketAddr,
        wp_trigger: WorkerPoolTrigger,
    ) -> Self {
        Self {
            local_agent_id,
            remote_addr: remote_socket_addr,
            workpool_trigger: Some(wp_trigger),
            ipc_receiver: None,
            ipc_sender: None,
        }
    }

    pub fn connect_primary(&mut self) {
        // Move worker pool trigger out of this object and into ipc signal receiver
        let workpool_trigger = self
            .workpool_trigger
            .take()
            .expect("missing WorkerPoolTrigger instance");

        // Connect to primary process
        let (trigger_stream, ready_stream) =
            connect_to_primary(self.local_agent_id, self.remote_addr);
        let sender = MioSocketSender::new(ready_stream);

        self.ipc_receiver = Some(IpcSignalReceiver::new(trigger_stream, workpool_trigger));
        self.sync_time();
        info!("Time synchronization with primary agent done");

        self.ipc_receiver.as_mut().unwrap().run();

        self.ipc_sender = Some(sender);
    }

    fn sync_time(&mut self) {
        let sync_info = self
            .ipc_receiver
            .as_mut()
            .expect("missing IPC sender")
            .receive_sync();
        timestamp::initialize_from(sync_info);
    }

    // Send ready signal using the given Activity ID
    pub fn send_ready(&mut self, activity_id: &ActivityId) -> Result<(), Error> {
        self.ipc_sender
            .as_mut()
            .expect("missing IPC sender")
            .send(Signal::Ready((*activity_id, timestamp())))
    }
}

pub fn run(mut agent: SecondaryAgent) {
    agent.run();
}

/// Common functionality used by secondary agents and recorders for connecting to the primary agent
///
/// Returns an incoming stream and an outgoing stream
pub fn connect_to_primary(
    local_agent_id: AgentId,
    remote_addr: SocketAddr,
) -> (TcpStream, TcpStream) {
    info!("Connecting to primary process at {}", remote_addr);
    let mut in_stream = loop {
        // Retry connecting in case of an error. This covers the scenario when the
        // primary process has not been started yet. We do use a std::net::TcpStream
        // instead of mio::net::TcpStream on purpose here and convert it accordingly
        // once the connection is established. Reason for that: polling the asynchronous
        // mio::net::TcpStream as suggested by mio's documentation turned out to behave
        // differently cross-platform-wise
        if let Ok(stream) = std::net::TcpStream::connect(remote_addr) {
            stream
                .make_nonblocking()
                .expect("failed to make stream non-blocking");
            break TcpStream::from_std(stream);
        } else {
            thread::sleep(Duration::from_millis(100));
        }
    };
    info!(
        "Connected to main process for incoming signals at {remote_addr}, sending 'hello_trigger'",
    );
    in_stream
        .set_nodelay(true)
        .unwrap_or_else(|e| panic!("setting nodelay for stream failed: {e:?}"));

    let mut sender = MioSocketSender::new(&mut in_stream);
    let hello_trigger = Signal::HelloTrigger(local_agent_id);
    sender
        .send(&hello_trigger)
        .unwrap_or_else(|e| panic!("failed to send 'hello_trigger': {:?}", e));

    let mut out_stream = TcpStream::connect(remote_addr).unwrap_or_else(|e| {
        panic!(
            "failed to connect ready stream to primary process at {}: {:?}",
            remote_addr, e
        )
    });
    info!("Connected to main process for outgoing signals at {remote_addr}, sending 'hello_ready'",);
    out_stream
        .set_nodelay(true)
        .unwrap_or_else(|e| panic!("setting nodelay for stream failed: {e:?}"));

    let mut sender = MioSocketSender::new(&mut out_stream);
    let hello_ready = Signal::HelloReady(local_agent_id);
    sender
        .send(&hello_ready)
        .unwrap_or_else(|e| panic!("failed to send 'hello_ready': {:?}", e));

    (in_stream, out_stream)
}
