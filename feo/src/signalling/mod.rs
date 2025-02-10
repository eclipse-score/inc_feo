// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

pub(crate) mod inter_proc_socket;
mod interface;
mod intra_proc_mpsc;
mod signals;

pub use inter_proc_socket::{
    MioMultiSocketReceiver, MioMultiSocketSender, MioSocketReceiver, MioSocketSender,
};
pub use interface::{Receiver, Sender};
pub use intra_proc_mpsc::{channel, IntraProcReceiver, IntraProcSender};
pub use signals::*;
