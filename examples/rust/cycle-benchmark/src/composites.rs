// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

use crate::activities::DummyActivity;
use crate::config::ActivityDependencies;
use feo::activity::{Activity, ActivityBuilder, ActivityIdAndBuilder};
use feo::ids::{ActivityId, WorkerId};
use feo_tracing::{instrument, tracing};
use std::collections::{HashMap, HashSet};

pub struct CompositeActivity {
    activity_id: ActivityId,
    /// ID as string (only used for tracing)
    _activity_id_str: String,
    activities: Vec<Box<dyn Activity>>,
}

impl CompositeActivity {
    pub fn build(
        activity_id: ActivityId,
        builders: Vec<ActivityIdAndBuilder>,
    ) -> Box<dyn Activity> {
        let activities: Vec<Box<dyn Activity>> =
            builders.into_iter().map(|idb| idb.1(idb.0)).collect();

        let composite = CompositeActivity {
            activity_id,
            _activity_id_str: u64::from(activity_id).to_string(),
            activities,
        };
        Box::new(composite)
    }
}

impl Activity for CompositeActivity {
    fn id(&self) -> ActivityId {
        self.activity_id
    }

    #[instrument(name = "Composite startup")]
    fn startup(&mut self) {
        for activity in &mut self.activities {
            activity.startup();
        }
    }

    #[instrument(name = "Composite step")]
    fn step(&mut self) {
        tracing::event!(tracing::Level::TRACE, id = self._activity_id_str);
        for activity in &mut self.activities {
            activity.step();
        }
    }

    #[instrument(name = "Composite shutdown")]
    fn shutdown(&mut self) {
        for activity in &mut self.activities {
            activity.shutdown();
        }
    }
}

impl core::fmt::Debug for CompositeActivity {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "CA{} [", self.activity_id)?;
        for activity in &self.activities {
            write!(f, "{},", activity.id())?;
        }
        write!(f, "]")
    }
}

/// Create a boxed builder of a composite of dummy activities
pub fn composite_builder(components: &[ActivityId]) -> Box<dyn ActivityBuilder> {
    let ids_builders: Vec<ActivityIdAndBuilder> = components
        .iter()
        .map(|id| {
            (
                *id,
                Box::new(DummyActivity::build) as Box<dyn ActivityBuilder>,
            )
        })
        .collect();
    Box::new(|id| CompositeActivity::build(id, ids_builders))
}

/// Determine chains of activities that could be put into a composite activity
pub fn find_composites(
    activity_deps: &ActivityDependencies,
    activity_worker_map: &HashMap<ActivityId, WorkerId>,
) -> Vec<Vec<ActivityId>> {
    let all_activities: HashSet<ActivityId> = activity_deps.keys().copied().collect();

    // Determine map of activities that have exactly one depending activity on the same worker;
    // the result is a map from dependency to depending activity
    let have_single_dependant: HashMap<ActivityId, ActivityId> = all_activities
        .iter()
        .map(|id| -> (ActivityId, Vec<ActivityId>) {
            (
                *id,
                activity_deps
                    .iter()
                    .filter(|(_, dependencies)| dependencies.contains(id))
                    .map(|(dependant, _)| *dependant)
                    .collect(),
            )
        })
        .filter(|(_, dependants)| dependants.len() == 1)
        .map(|(id, dependants)| (id, dependants[0]))
        .filter(|(a1, a2)| activity_worker_map[a1] == activity_worker_map[a2])
        .collect();

    // Determine set of activities that depend on at exactly one activity on the same worker
    let have_single_dependency: HashSet<ActivityId> = all_activities
        .iter()
        .filter(|id| 1 == activity_deps.get(id).expect("missing activity id").len())
        .map(|id| (id, &activity_deps[id][0]))
        .filter(|(a1, a2)| activity_worker_map[a1] == activity_worker_map[*a2])
        .map(|(id, _)| *id)
        .collect();

    // Determine activity chains
    let mut chains = Vec::<Vec<ActivityId>>::new();

    // Loop over potential chain start activities (having a single dependant)
    for chain_start in have_single_dependant.keys() {
        // skip activities depending on a single other activity having a single dependant
        // => not first in chain, will be included in another chain
        if have_single_dependency.contains(chain_start) {
            let dependency = activity_deps
                .get(chain_start)
                .expect("missing dependencies")[0];
            if have_single_dependant.contains_key(&dependency) {
                continue;
            }
        }

        let mut current = chain_start;
        let mut chain = Vec::<ActivityId>::new();

        // Loop until end of chain
        loop {
            // Add activity to the chain
            chain.push(*current);

            // If the current activity does not have a single dependant,
            // the chain ends here
            if !have_single_dependant.contains_key(current) {
                break;
            }

            // Get dependant
            let dependant = have_single_dependant
                .get(current)
                .expect("missing dependant");

            // Check, if dependant has a single dependency;
            // otherwise, it is not part of the chain and the chain ends
            if !have_single_dependency.contains(dependant) {
                break;
            }

            // Continue building the chain
            current = dependant;
        }

        // If the current chain has a length > 1, store it
        if chain.len() > 1 {
            chains.push(chain);
        }
    }

    chains
}
