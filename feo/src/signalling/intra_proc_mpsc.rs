// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

use super::{Receiver, Sender};
use crate::error::Error;
use crate::error::Error::Channel;
use std::sync::mpsc;

pub fn channel<T>() -> (IntraProcSender<T>, IntraProcReceiver<T>) {
    let (sender, receiver) = mpsc::channel();
    (
        IntraProcSender::new(sender),
        IntraProcReceiver::new(receiver),
    )
}

pub struct IntraProcReceiver<T> {
    receiver: mpsc::Receiver<T>,
}

impl<T> IntraProcReceiver<T> {
    pub fn new(mpsc_rec: mpsc::Receiver<T>) -> IntraProcReceiver<T> {
        IntraProcReceiver { receiver: mpsc_rec }
    }
}

impl<T: Send> Receiver<T> for IntraProcReceiver<T> {
    fn recv(&mut self) -> Result<T> {
        self.receiver
            .recv()
            .map_err(|_| Channel("failed to receive signal"))
    }
}

pub struct IntraProcSender<T> {
    sender: mpsc::Sender<T>,
}

impl<T> IntraProcSender<T> {
    pub fn new(mpsc_snd: mpsc::Sender<T>) -> IntraProcSender<T> {
        IntraProcSender { sender: mpsc_snd }
    }
}

impl<T> Clone for IntraProcSender<T> {
    fn clone(&self) -> IntraProcSender<T> {
        IntraProcSender {
            sender: self.sender.clone(),
        }
    }
}

impl<T: Send> Sender<T> for IntraProcSender<T> {
    fn send(&mut self, t: T) -> Result<()> {
        self.sender
            .send(t)
            .map_err(|_| Channel("failed to send signal"))
    }
}

type Result<T, E = Error> = std::result::Result<T, E>;
