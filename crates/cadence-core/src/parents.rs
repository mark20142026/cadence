//! Group parents and linked objects: how advanced follow (3016) and the area triggers
//! (object type 45, 3006–3015) treat a target group's objects as clusters that move as one.
//!
//! The game builds this once per level (`FUN_1000d64d0`, run by `updateSpecialGroupData` after
//! `sortStickyGroups`) for every target group of those triggers and each combination of their
//! "ignore group parent" (280) and "ignore linked" (281) flags that the level uses, under the
//! key `getSpecialKey(g, 280, 281)`:
//!
//! - every object of the group starts in its linked group's cluster (key 108; 0 = none);
//! - unless 280: an object of the group that is a group parent (key 274, `m_parentGroups`)
//!   pulls the objects of the groups it's the parent of into cluster −child (only the group's
//!   own parent and the group itself when the group has an explicit parent). Those objects
//!   leave their linked clusters; the cluster's leader is the first object of the child group
//!   that is the parent object itself or has the group parent (34) / area parent (279) flag,
//!   preferring area parents; its "parent" the same, preferring group parents;
//! - unless 281: a linked cluster's leader is its first object with the group parent or area
//!   parent flag (preferring area parents) — a linked cluster without one has none;
//! - no leader at all: the plain group is used. Else the objects of the group split into the
//!   ones moved alone (no cluster, or a cluster without a leader), the leaders, and per leader
//!   the objects that move with it (a linked group's every object, or the child group's) and
//!   its parent object (area rotate / scale turn about it). Members that aren't leaders are
//!   left out: their leader moves them.
//!
//! `tryGetMainObject(g)` (the center or target of most triggers): the first parent of g (274;
//! a "+" item makes that object the first), else the group's only object, else none.
//!
//! Spawn remaps: the same function gathers every spawn trigger's remap pairs (1268 key 442,
//! from → to) and gives each target group's remapped groups the same flag combinations
//! (following remaps of remaps), so a trigger spawned with a remap finds the clusters of the
//! group it was remapped to.

use crate::level::LevelString;
use std::collections::HashMap;

/// A target group's clusters (`m_targetGroupsArray` entries A, B, D, E).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Clusters {
    /// Objects moved alone (level orders, the group's order).
    pub alone: Vec<u32>,
    /// Cluster leaders, the group's order.
    pub leaders: Vec<u32>,
    /// Per leader: the objects that move with it (itself included when it's in the cluster).
    pub members: HashMap<u32, Vec<u32>>,
    /// Per leader: the cluster's parent object (rotation / scale center of area effects).
    pub parent: HashMap<u32, u32>,
}

/// The level's group parents, linked groups and the clusters of the triggers' target groups.
#[derive(Clone, Debug, Default)]
pub struct GroupLinks {
    /// Group → its parent objects (key 274), the first one is `tryGetMainObject`'s.
    pub parents_of: HashMap<i32, Vec<u32>>,
    /// (group, ignore parent, ignore linked) → its clusters (`None`: the plain group).
    clusters: HashMap<(i32, bool, bool), Option<std::sync::Arc<Clusters>>>,
}

/// The parent groups of key 274 ("." separated; "+" in front: put the object first among that
/// group's parents if it's listed already).
fn parent_groups(text: &str) -> impl Iterator<Item = (i32, bool)> + '_ {
    text.split('.').filter_map(|item| {
        let front = item.starts_with('+');
        item.trim_start_matches('+').parse::<i32>().ok().filter(|&g| g > 0).map(|g| (g, front))
    })
}

impl GroupLinks {
    pub fn new(level: &LevelString) -> GroupLinks {
        let objs = &level.objects;
        // the target groups and their flag combinations
        let mut targets: Vec<(i32, bool, bool)> = Vec::new();
        for o in objs {
            let id = o.id();
            if id == 3016 || (3006..=3015).contains(&id) {
                let g = o.get_i32(51).unwrap_or(0);
                if g > 0 {
                    let key = (g, o.get(280) == Some("1"), o.get(281) == Some("1"));
                    if !targets.contains(&key) {
                        targets.push(key);
                    }
                }
            }
        }
        // spawn remaps: from → its "to" groups, over every spawn trigger; the remapped groups get
        // the flags of the groups they're remapped from
        let mut remaps: HashMap<i32, Vec<i32>> = HashMap::new();
        for o in objs.iter().filter(|o| o.id() == 1268) {
            let pairs: Vec<i32> = o.get(442).map(|r| r.split('.').filter_map(|n| n.parse().ok()).collect()).unwrap_or_default();
            for c in pairs.chunks_exact(2) {
                let to = remaps.entry(c[0]).or_default();
                if !to.contains(&c[1]) {
                    to.push(c[1]);
                }
            }
        }
        let mut i = 0;
        while i < targets.len() {
            let (g, ignore_parent, ignore_linked) = targets[i];
            for &to in remaps.get(&g).map(|v| v.as_slice()).unwrap_or(&[]) {
                if to > 0 && !targets.contains(&(to, ignore_parent, ignore_linked)) {
                    targets.push((to, ignore_parent, ignore_linked));
                }
            }
            i += 1;
        }
        // parents (`loadGroupParentsFromString`, at each object's load: level order)
        let mut parents_of: HashMap<i32, Vec<u32>> = HashMap::new();
        let mut children: HashMap<u32, Vec<i32>> = HashMap::new();
        for (i, o) in objs.iter().enumerate() {
            let Some(text) = o.get(274) else { continue };
            for (g, front) in parent_groups(text) {
                // `FUN_1000f5770` appends a new parent; "+" (`FUN_1000f58d8`) moves one already
                // listed to the front
                let list = parents_of.entry(g).or_default();
                if !list.contains(&(i as u32)) {
                    list.push(i as u32);
                } else if front {
                    list.retain(|&p| p != i as u32);
                    list.insert(0, i as u32);
                }
                let c = children.entry(i as u32).or_default();
                if !c.contains(&g) {
                    c.push(g);
                }
            }
        }
        let mut links = GroupLinks { parents_of, ..Default::default() };
        if targets.is_empty() {
            return links;
        }
        // every group's objects (only groups that matter are kept below)
        let mut all: HashMap<i32, Vec<u32>> = HashMap::new();
        let mut linked: HashMap<i32, Vec<u32>> = HashMap::new();
        for (i, o) in objs.iter().enumerate() {
            for g in o.groups() {
                all.entry(g).or_default().push(i as u32);
            }
            if let Some(l) = o.get_i32(108).filter(|&l| l > 0) {
                linked.entry(l).or_default().push(i as u32);
            }
        }
        let flag = |i: u32, k: u32| objs[i as usize].get_i32(k).is_some_and(|v| v != 0);
        let link_of = |i: u32| objs[i as usize].get_i32(108).filter(|&l| l > 0).unwrap_or(0);
        for &(g, ignore_parent, ignore_linked) in &targets {
            let members = all.get(&g).cloned().unwrap_or_default();
            // cluster of each object (linked group, or −child group), insertion-ordered
            let mut cluster: HashMap<u32, i32> = members.iter().map(|&o| (o, link_of(o))).collect();
            let mut leader: HashMap<i32, u32> = HashMap::new();
            let mut parent: HashMap<i32, u32> = HashMap::new();
            // a candidate replaces the current pick when it has the preferred flag and the
            // current one hasn't it (or the candidate is the parent object itself)
            let offer = |map: &mut HashMap<i32, u32>, k: i32, o: u32, prefer: u32, is_obj: bool| match map.get(&k) {
                None => {
                    map.insert(k, o);
                }
                Some(&cur) => {
                    if (!flag(cur, prefer) || is_obj) && flag(o, prefer) {
                        map.insert(k, o);
                    }
                }
            };
            if !ignore_parent {
                let explicit = links.parents_of.get(&g).and_then(|p| p.first()).copied();
                for &obj in &members {
                    let Some(kids) = children.get(&obj) else { continue };
                    for &c in kids {
                        if !(explicit.is_none() || (explicit == Some(obj) && c == g)) {
                            continue;
                        }
                        for &o in all.get(&c).map(|v| v.as_slice()).unwrap_or(&[]) {
                            cluster.remove(&o);
                            if flag(o, 34) || flag(o, 279) || o == obj {
                                offer(&mut leader, -c, o, 279, o == obj);
                                offer(&mut parent, -c, o, 34, o == obj);
                            }
                        }
                        if let Some(&l) = leader.get(&-c) {
                            cluster.insert(l, -c);
                        }
                    }
                }
            }
            if !ignore_linked {
                for &o in &members {
                    if let Some(&k) = cluster.get(&o) {
                        if k > 0 && (flag(o, 34) || flag(o, 279)) {
                            offer(&mut leader, k, o, 279, false);
                            offer(&mut parent, k, o, 34, false);
                        }
                    }
                }
            }
            let entry = if leader.is_empty() {
                None
            } else {
                let mut c = Clusters::default();
                for &o in &members {
                    let Some(&k) = cluster.get(&o) else { continue };
                    if k == 0 {
                        c.alone.push(o);
                        continue;
                    }
                    match leader.get(&k) {
                        Some(&l) if l == o => {
                            c.leaders.push(o);
                            let moved = if k < 0 { all.get(&-k).cloned() } else { linked.get(&k).cloned() };
                            c.members.insert(o, moved.unwrap_or_default());
                            if let Some(&p) = parent.get(&k) {
                                c.parent.insert(o, p);
                            }
                        }
                        Some(_) => {}
                        None => c.alone.push(o),
                    }
                }
                Some(std::sync::Arc::new(c))
            };
            links.clusters.insert((g, ignore_parent, ignore_linked), entry);
        }
        links
    }

    /// The clusters of a target group with these flags (`None`: use the plain group).
    pub fn clusters(&self, group: i32, ignore_parent: bool, ignore_linked: bool) -> Option<&std::sync::Arc<Clusters>> {
        self.clusters.get(&(group, ignore_parent, ignore_linked)).and_then(|c| c.as_ref())
    }

    /// Every object some cluster moves (they can move even when not in a moving group).
    pub fn cluster_objects(&self) -> impl Iterator<Item = u32> + '_ {
        self.clusters.values().flatten().flat_map(|c| c.members.values().flatten().copied())
    }

    /// The first parent object of a group (key 274), if any.
    pub fn parent_of(&self, group: i32) -> Option<u32> {
        self.parents_of.get(&group).and_then(|p| p.first()).copied()
    }

    /// Groups that have a parent (their members must be known to the world).
    pub fn parent_groups(&self) -> impl Iterator<Item = i32> + '_ {
        self.parents_of.keys().copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn links(objs: &str) -> GroupLinks {
        GroupLinks::new(&LevelString::parse(&format!("kA2,0;{objs}")))
    }

    #[test]
    fn linked_cluster_needs_a_flagged_leader() {
        // 0..2 linked 1 (0 has the group parent flag), 3 alone; 4, 5 linked 2 without a flag
        let l = links(
            "1,211,57,10,108,1,34,1;1,211,57,10,108,1;1,211,57,10,108,1;1,211,57,10;1,211,57,11,108,2;1,211,57,11,108,2;\
             1,3016,51,10;1,3016,51,11",
        );
        let c = l.clusters(10, false, false).unwrap();
        assert_eq!(c.leaders, vec![0]);
        assert_eq!(c.alone, vec![3]);
        assert_eq!(c.members[&0], vec![0, 1, 2]);
        assert_eq!(c.parent[&0], 0);
        assert!(l.clusters(11, false, false).is_none());
    }

    #[test]
    fn group_parent_pulls_its_child_group() {
        // 0 ∈ 30, 31 is the parent of 31; 1 ∈ 30, 31; 2 ∈ 31; 3 ∈ 30
        let l = links("1,211,57,30.31,274,31;1,211,57,30.31;1,211,57,31;1,211,57,30;1,3016,51,30;1,3016,51,30,280,1");
        let c = l.clusters(30, false, false).unwrap();
        assert_eq!(c.leaders, vec![0]);
        assert_eq!(c.alone, vec![3]);
        assert_eq!(c.members[&0], vec![0, 1, 2]);
        // ignoring parents: no clusters at all
        assert!(l.clusters(30, true, false).is_none());
    }

    #[test]
    fn explicit_parent_makes_the_group_one_cluster() {
        let l = links("1,211,57,20;1,211,57,20;1,211,57,20,274,20;1,211,57,20;1,3006,51,20");
        let c = l.clusters(20, false, false).unwrap();
        assert_eq!(c.leaders, vec![2]);
        assert!(c.alone.is_empty());
        assert_eq!(c.members[&2], vec![0, 1, 2, 3]);
        assert_eq!(l.parent_of(20), Some(2));
    }

    #[test]
    fn area_parent_is_the_preferred_leader() {
        // linked 3: 0 has the group parent flag, 1 the area parent flag
        let l = links("1,211,57,5,108,3,34,1;1,211,57,5,108,3,279,1;1,3006,51,5");
        let c = l.clusters(5, false, false).unwrap();
        assert_eq!(c.leaders, vec![1]);
        assert_eq!(c.parent[&1], 0);
    }

    #[test]
    fn remapped_target_groups_get_clusters() {
        // advanced follow on group 5 (no members); a spawn remaps 5 → 6 (a linked cluster) and
        // 6 → 7 (a parent cluster) in another trigger
        let l = links(
            "1,211,57,6,108,1,34,1;1,211,57,6,108,1;1,211,57,7,274,7;1,211,57,7;\
             1,3016,51,5,281,0;1,1268,51,9,442,5.6;1,1268,51,9,442,6.7",
        );
        let c = l.clusters(6, false, false).unwrap();
        assert_eq!(c.leaders, vec![0]);
        assert_eq!(c.members[&0], vec![0, 1]);
        let c = l.clusters(7, false, false).unwrap();
        assert_eq!(c.leaders, vec![2]);
        // not for flag combinations the source group doesn't use
        assert!(l.clusters(6, true, false).is_none());
    }

    #[test]
    fn plus_puts_a_parent_first() {
        let l = links("1,211,57,7,274,7;1,211,57,7,274,+7");
        assert_eq!(l.parent_of(7), Some(0));
        let l = links("1,211,57,7,274,7;1,211,57,7,274,7.+7");
        assert_eq!(l.parent_of(7), Some(1));
    }
}
