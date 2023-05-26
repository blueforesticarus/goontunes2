//! calculate a playlist diff
//! Spotify limits the # of alterations to a playlist per request. This file contains functions to take the diff between
//! 2 track lists and intelligently sequence api requests to get from one to the other.

/* Ported from Go (eww, gross)
func (self *ServicePlaylist) Update(p *Playlist) error {
    fmt.Printf("Begin sync of %s <-> %s\n", self.ID, p.Name)

    //assume p is correct playlist, and p.rebuild called
    err := self.scan()
    if err != nil {
        return fmt.Errorf("Abort playlist update. %v \n", err)
    }

    if len(p.tracks) == 0 {
        return fmt.Errorf("Abort playlist update. %v \n", "refusing to sync a empty playlist")
    }

    current := make([]string, len(self.cache.TracksIDs))
    for i, v := range self.cache.TracksIDs {
        current[i] = v
    }

    //get just the strings, filter things missing spot info
    target := make([]string, 0, len(p.tracks))
    for _, v := range p.tracks {
        if v == nil || v.IDMaps == nil {
            //bug
            return fmt.Errorf("Abort playlist sync, error with track")
        }

        id := self.service.Get_Track_Id(v)
        if id == "" {
            //no id for this track on this service
            continue
        }
        if self.NoCrossRef {
            panic("unimplemented")
            //NOTE: not sure clean way to add this functionality
            //if the id from the entry is not the same, then it is a cross referenced track
        }

        if self.NoDelete {
            if util.Contains_a_fucking_string(target, id) ||
                util.Contains_a_fucking_string(current, id) {
                // we skip duplicates for nodelete, kinda have to
                continue
            }
        }
        target = append(target, id)
    }

    if self.NoDelete {
        //for nodelete we target the playlist plus current
        //NOTE: this is prepending, new stuff will be at the top
        target = append(target, current...)
    }

    fmt.Printf("Attempting to sync %d/%d tracks\n", len(target), len(p.tracks))

    //compute delta
    rm_list := make([]Pl_Rm, 0, 100)
    ins_list := make([]Pl_Ins, 0, 100)

    sm := difflib.NewMatcher(current, target)
    for _, v := range sm.GetOpCodes() {
        if v.Tag == 'd' || v.Tag == 'r' {
            for i := v.I1; i < v.I2; i++ {
                a := Pl_Rm{current[i], i}
                rm_list = append(rm_list, a)
            }
        }

        if v.Tag == 'i' || v.Tag == 'r' {
            b := Pl_Ins{target[v.J1:v.J2], v.J1}
            ins_list = append(ins_list, b)
        }
    }

    //sort strings (don't rely on snapshot)
    sort.Slice(ins_list, func(i, j int) bool {
        return ins_list[i].i < ins_list[j].i //insert last first
    })

    sort.Slice(rm_list, func(i, j int) bool {
        return rm_list[i].i > rm_list[j].i //sort reverse
    })

    if len(ins_list) == 0 && len(rm_list) == 0 {
        fmt.Printf("playlist is already correct\n")
    } else {
        //delete tracks
        if len(rm_list) > 0 {
            n := self.service.Playlist_DeleteTracks(self.ID, rm_list)
            fmt.Printf("updated playlist %s, %d deleted\n", self.ID, n)
        }
        if len(ins_list) > 0 {
            n := self.service.Playlist_InsertTracks(self.ID, ins_list)
            fmt.Printf("SPOTIFY: updated playlist %s, %d inserted\n", self.ID, n)
        }
    }

    err = self.scan()
    if err == nil {
        err = self.check(target)
    }

    //Description
    now := time.Now()
    desc := fmt.Sprintf("%s (Updated: %s)", p.Description, now.Format(time.UnixDate))
    self.service.Playlist_Description(self.ID, desc)

    return err
}
 */

use std::collections::{BTreeMap, HashMap};

use itertools::Itertools;

/// Plan out api requests for updating playlist
pub fn sequence<T: Eq + core::hash::Hash + Ord + Clone>(
    before: Vec<T>,
    after: Vec<T>,
) -> Vec<Actions<T>> {
    let max_n = 100; // max changes per op
    let delete_positional = true; // can we specify index in delete?
    let mut actions = Vec::new();

    // Diffs are hard. We know the index of things in the new struct, but not in the steps inbetween.
    // Deletes first, then recompute diff, then Adds.
    // Keep in mind we do not have the ability specify snapshot for Adds
    let mut simulated = before.clone();

    let diff = similar::capture_diff_slices(similar::Algorithm::Patience, &before, &after);
    for d in diff.iter().rev() {
        match d {
            similar::DiffOp::Replace {
                old_index, old_len, ..
            }
            | similar::DiffOp::Delete {
                old_index, old_len, ..
            } => {
                let index = *old_index..(old_index + old_len);
                let a = before[index.clone()]
                    .iter()
                    .enumerate()
                    .chunks(max_n)
                    .into_iter()
                    .map(|chunk| match delete_positional {
                        true => {
                            let chunk = chunk.map(|(i, t)| (i, t.clone())).collect_vec();
                            Actions::Delete(chunk, Default::default())
                        }
                        false => {
                            let chunk = chunk.map(|(i, t)| t.clone()).collect_vec();
                            Actions::DeleteAll(chunk, Default::default())
                        }
                    })
                    .collect_vec();
                actions.extend(a);

                let removed = simulated.drain(index.clone()).collect_vec();
                assert!(removed == before[index], "diffs need to be sorted");
            }
            _ => { /* skip inserts on first pass */ }
        }
    }

    let diff = similar::capture_diff_slices(similar::Algorithm::Patience, &simulated, &after);
    for d in diff.iter().rev() {
        match d {
            similar::DiffOp::Insert {
                old_index,
                new_index,
                new_len,
                ..
            } => {
                let new_slice = *new_index..(new_index + new_len);
                let elements = after[new_slice].to_owned();
                actions.push(Actions::Add(elements.clone(), *old_index));

                simulated.splice(old_index..old_index, elements);

                let new_slice = *new_index..(new_index + new_len);
                assert!(simulated[new_slice.clone()] == after[new_slice]);
            }
            similar::DiffOp::Equal { .. } => {}
            _ => {
                unreachable!("should not have deletions on second pass")
            }
        }
    }

    assert!(simulated == after);

    // Check if current plan is worse (more api requests) than clearing and rebuilding the whole playlist.
    // Note: if actually using snapshot actions, then correct actions count.
    let worst_case = (after.len() + max_n - 1) / max_n;
    if actions.len() >= worst_case {
        actions.clear();
        simulated.clear();

        let items = after.iter().chunks(max_n);
        let mut items = items.into_iter().map(|f| f.cloned().collect_vec());

        // First action is replace, which clears the playlist
        let mut v = items.next().unwrap_or_default();
        actions.push(Actions::Replace(v.clone()));
        simulated.append(&mut v);

        // If playlist more than 100 tracks then we need further add commands
        for mut v in items {
            actions.push(Actions::Append(v.clone()));
            simulated.append(&mut v);
        }

        assert!(simulated == after);
    }

    actions
}

pub type SnapshotKey = usize;

// Available operations:
/// add up to 100 ids (position for group NO SNAPSHOT)
/// move up to 100 contiguous ids

pub enum Actions<T> {
    /// unused: tell spotify module to record last actions snapshot so it can be refered to as SnapshotKey in later requests.
    Snapshot(SnapshotKey),

    /// add up to 100 ids to end of playlist
    Append(Vec<T>),

    /// insert up to 100 ids at index
    Add(Vec<T>, usize),

    /// delete up to 100 ids (undocumented positional disambiguation)
    Delete(Vec<(usize, T)>, SnapshotKey),

    /// delete up to 100 ids (deletes all occurances of uri)
    DeleteAll(Vec<T>, SnapshotKey),

    /// replace (clear) the whole playlist with up to 100 ids
    Replace(Vec<T>),

    /// TODO
    Move {
        index: usize,
        count: usize,
        to: usize,
    },
}

#[cfg(test)]
mod test {
    use itertools::Itertools;

    use super::sequence;

    #[test]
    fn test_delete_append() {
        let before = vec!["red", "blue", "green", "yellow"];

        let after = vec!["blue", "green", "yellow", "black"];
    }

    #[test]
    fn test_big_playlist() {
        let mut before = (0..2000).map(|v| v.to_string()).collect_vec();
        before.splice(300..400, (400..500).map(|v| v.to_string())); // some repeat values
        before.splice(203..205, (0..3).map(|_| "203".to_string())); // some contiguous repeat values

        let mut after = before.clone();
        after.extend((3000..3234).map(|v| v.to_string())); // big append
        after.drain(1500..1892); // big delete
        after.drain(1411..1414); // small delete
        after.remove(1306); // single delete
        after.remove(1300); // single delete

        after.remove(444); // single delete (repeat id)
        after.drain(255..367); // repeat id delete span
        after.drain(203..204); // delete (repeat id, both)

        // sequence has its own asserts and simulation, test passes if it doesnt panic
        sequence(before, after);
    }
}
