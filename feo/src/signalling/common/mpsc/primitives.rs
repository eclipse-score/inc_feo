// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

//! Primitive building blocks of mpsc channel signalling implementation

use crate::error::Error;
use core::fmt;
use core::time::Duration;
use std::sync::mpsc;
use std::sync::mpsc::RecvTimeoutError;

const MPSC_CHANNEL_BOUND: usize = 128;

pub(crate) fn channel<T: fmt::Debug>() -> (Sender<T>, Receiver<T>) {
    let (mpsc_sender, mpsc_receiver) = mpsc::sync_channel(MPSC_CHANNEL_BOUND);
    let sender = Sender::new(mpsc_sender);
    let receiver = Receiver::new(mpsc_receiver);
    (sender, receiver)
}

pub(crate) struct Receiver<T: fmt::Debug + 'static> {
    pub(crate) receiver: mpsc::Receiver<T>,
}

impl<T: fmt::Debug + 'static> Receiver<T> {
    pub(crate) fn new(receiver: mpsc::Receiver<T>) -> Self {
        Self { receiver }
    }

    pub fn receive(&mut self, timeout: Duration) -> Result<Option<T>, Error> {
        match self.receiver.recv_timeout(timeout) {
            Ok(v) => Ok(Some(v)),
            Err(err) => match err {
                RecvTimeoutError::Timeout => Ok(None),
                _ => Err(Error::Channel("channel closed")),
            },
        }
    }
}

#[derive(Clone)]
pub(crate) struct Sender<T: fmt::Debug + 'static> {
    pub(crate) sender: mpsc::SyncSender<T>,
}

impl<T: fmt::Debug + 'static> Sender<T> {
    pub(crate) fn new(sender: mpsc::SyncSender<T>) -> Self {
        Self { sender }
    }

    pub fn send(&mut self, t: T) -> Result<(), Error> {
        self.sender
            .send(t)
            .map_err(|_| Error::Channel("channel closed"))?;
        Ok(())
    }
}
