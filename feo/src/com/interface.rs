// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

use std::any::Any;
use std::marker::PhantomData;

#[derive(Debug)]
/// Incoming data provided to an [Activity](crate::activity::Activity)
pub struct Input<T, U> {
    pub(crate) inner: U,
    pub(crate) _type: PhantomData<T>,
}

#[derive(Debug)]
/// Container type for incoming data
pub struct InputGuard<T, U> {
    pub(crate) inner: U,
    pub(crate) _type: PhantomData<T>,
}

#[derive(Debug)]
/// Outgoing data written by an [Activity](crate::activity::Activity)
pub struct Output<T, U> {
    pub(crate) inner: U,
    pub(crate) _type: PhantomData<T>,
}

#[derive(Debug)]
/// Container type for outgoing data
pub struct OutputGuard<T, U> {
    pub(crate) inner: U,
    pub(crate) _type: PhantomData<T>,
}

#[must_use = "keep me alive until activities are created"]
/// Opaque handle of a topic.
///
/// This must be kept alive aftere topic initialization until the activities are started.
pub struct TopicHandle {
    _inner: Box<dyn Any>,
}

impl<T: 'static> From<Box<T>> for TopicHandle {
    fn from(value: Box<T>) -> Self {
        TopicHandle { _inner: value }
    }
}
