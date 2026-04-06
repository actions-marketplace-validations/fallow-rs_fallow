//! Phase 3: BFS reachability from entry points.

use std::collections::VecDeque;

use fixedbitset::FixedBitSet;

use super::ModuleGraph;

impl ModuleGraph {
    fn collect_reachable(
        &self,
        entry_points: &rustc_hash::FxHashSet<fallow_types::discover::FileId>,
        total_capacity: usize,
    ) -> FixedBitSet {
        let mut visited = FixedBitSet::with_capacity(total_capacity);
        let mut queue = VecDeque::new();

        for &ep_id in entry_points {
            if (ep_id.0 as usize) < total_capacity {
                visited.insert(ep_id.0 as usize);
                queue.push_back(ep_id);
            }
        }

        while let Some(file_id) = queue.pop_front() {
            if (file_id.0 as usize) >= self.modules.len() {
                continue;
            }
            let module = &self.modules[file_id.0 as usize];
            for edge in &self.edges[module.edge_range.clone()] {
                let target_idx = edge.target.0 as usize;
                if target_idx < total_capacity && !visited.contains(target_idx) {
                    visited.insert(target_idx);
                    queue.push_back(edge.target);
                }
            }
        }

        visited
    }

    /// Mark modules reachable from overall, runtime, and test entry points via BFS.
    pub(super) fn mark_reachable(
        &mut self,
        entry_points: &rustc_hash::FxHashSet<fallow_types::discover::FileId>,
        runtime_entry_points: &rustc_hash::FxHashSet<fallow_types::discover::FileId>,
        test_entry_points: &rustc_hash::FxHashSet<fallow_types::discover::FileId>,
        total_capacity: usize,
    ) {
        let visited = self.collect_reachable(entry_points, total_capacity);
        let runtime_visited = self.collect_reachable(runtime_entry_points, total_capacity);
        let test_visited = self.collect_reachable(test_entry_points, total_capacity);

        for (idx, module) in self.modules.iter_mut().enumerate() {
            module.is_reachable = visited.contains(idx);
            module.is_runtime_reachable = runtime_visited.contains(idx);
            module.is_test_reachable = test_visited.contains(idx);
        }
    }
}
