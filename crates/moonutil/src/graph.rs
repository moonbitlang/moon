// moon: The build system and package manager for MoonBit.
// Copyright (C) 2024 International Digital Economy Academy
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
//
// For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

use petgraph::graph::NodeIndex;

pub fn get_example_cycle(
    m: &petgraph::graph::DiGraph<String, usize>,
    n: petgraph::prelude::NodeIndex,
) -> Vec<petgraph::prelude::NodeIndex> {
    // the parent of each node in the spanning tree
    let mut spanning_tree = vec![NodeIndex::default(); m.capacity().0];
    // we find a cycle via dfs from our starting point
    let res = petgraph::visit::depth_first_search(&m, [n], |ev| match ev {
        petgraph::visit::DfsEvent::TreeEdge(parent, n) => {
            spanning_tree[n.index()] = parent;
            petgraph::visit::Control::Continue
        }
        petgraph::visit::DfsEvent::BackEdge(u, v) => {
            if v == n {
                // Cycle found! Bail out of the search.
                petgraph::visit::Control::Break(u)
            } else {
                // This is not the cycle we are looking for.
                petgraph::visit::Control::Continue
            }
        }
        _ => {
            // Continue the search.
            petgraph::visit::Control::Continue
        }
    });
    let res = res.break_value().expect("The cycle should be found");
    let mut cycle = vec![n];
    let mut curr_node = res;
    loop {
        cycle.push(curr_node);
        if curr_node == n {
            break;
        }
        curr_node = spanning_tree[curr_node.index()]; // get parent
    }
    cycle.reverse(); // the cycle was pushed in reverse order
    cycle
}
