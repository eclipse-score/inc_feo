// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

use crate::activity::ActivityId;
use crate::com::TopicHandle;

pub type Topic = &'static str;

#[derive(Debug, Default, Clone, Copy)]
/// Describes the direction of the data flow for one topic of one component
pub enum Direction {
    /// incoming / received data
    #[default]
    Incoming,

    /// outgoing / sent data
    Outgoing,
}

/// Specification of a topic's peers and init function
pub struct TopicSpecification {
    /// Peers with [ActivityId] and communication [Direction] for this topic
    pub peers: Vec<(ActivityId, Direction)>,
    /// Function to initialize this topic with the number of writers and readers as arguments
    pub init_fn: Box<dyn FnOnce(usize, usize) -> TopicHandle>,
}
