// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

//! Topic based communication

mod interface;

#[cfg(feature = "ipc_iceoryx2")]
mod backend_iceoryx2;

#[cfg(feature = "ipc_iceoryx2")]
use ::iceoryx2::{
    port::{publisher::Publisher, subscriber::Subscriber},
    service::ipc,
};
pub use interface::{Input, InputGuard, Output, OutputGuard, TopicHandle};

#[cfg(feature = "ipc_iceoryx2")]
pub type ActivityInput<T> = Input<T, Subscriber<ipc::Service, T, ()>>;
#[cfg(feature = "ipc_iceoryx2")]
pub type ActivityOutput<T> = Output<T, Publisher<ipc::Service, T, ()>>;

#[cfg(feature = "ipc_iceoryx2")]
pub use backend_iceoryx2::init_topic;
