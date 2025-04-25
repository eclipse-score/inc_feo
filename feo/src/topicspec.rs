// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

//! Specification of a topic's peers and init function

use crate::ids::ActivityId;
use alloc::boxed::Box;
use alloc::vec::Vec;
use core::fmt;
use feo_com::interface::{
    init_topic_primary, init_topic_secondary, ComBackendTopicPrimaryInitialization,
    ComBackendTopicSecondaryInitialization, Topic, TopicHandle,
};

#[derive(Debug, Default, Clone, Copy, Eq, PartialEq)]
/// Describes the direction of the data flow for one topic of one component
pub enum Direction {
    /// incoming / received data
    #[default]
    Incoming,

    /// outgoing / sent data
    Outgoing,
}

/// Specification of a topic's backend and peers
pub struct TopicSpecification<'a> {
    /// The topic
    pub topic: Topic<'a>,
    /// Peers with [ActivityId] and communication [Direction] for this topic
    pub peers: Vec<(ActivityId, Direction)>,
    /// Function to initialize this topic with the number of writers and readers as arguments
    pub init_primary_fn: Box<dyn FnOnce(&ComBackendTopicPrimaryInitialization) -> TopicHandle>,
    pub init_secondary_fn: Box<dyn FnOnce(&ComBackendTopicSecondaryInitialization) -> TopicHandle>,
}

impl<'a> TopicSpecification<'a> {
    pub fn new<T: Default + fmt::Debug + 'static>(
        topic: Topic<'a>,
        peers: Vec<(ActivityId, Direction)>,
    ) -> Self {
        let init_primary_fn = Box::new(init_topic_primary::<T>);
        let init_secondary_fn = Box::new(init_topic_secondary::<T>);
        Self {
            topic,
            peers,
            init_primary_fn,
            init_secondary_fn,
        }
    }
}
