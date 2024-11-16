//! Calculate a playlist diff and sequence changes
//! Spotify limits the # of tracks that can be added/removed in a single api request.
//! This file contains functions to take the diff between two track lists and generate an efficient list of api requests to get from one to the other.

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

use itertools::Itertools;
use similar::{algorithms::IdentifyDistinct, capture_diff, capture_diff_deadline};

#[derive(Debug, Clone, Copy)]
/// Configures how playlist updates are sequenced
pub struct SequenceOptions {
    /// Whether to disambiguate indices for deletions
    /// if false, only ids are specified and all occurances will be removed, possibly requiring re-adding them to other locations
    /// this exists because indexed delete is an undocumented feature of the spotify api, which we might not want to depend on
    /// see: https://community.spotify.com/t5/Spotify-for-Developers/How-delete-one-or-more-elements-from-playlist/td-p/5185630
    pub delete_positional: bool,

    /// max tracks per Add / Delete api request
    pub max_n: usize,
}

impl Default for SequenceOptions {
    fn default() -> Self {
        Self {
            delete_positional: false,
            max_n: 100, // spotify max is 100
        }
    }
}

/// Plan out api requests for updating playlist
pub fn sequence<T: Eq + core::hash::Hash + Clone>(
    before: Vec<T>,
    after: Vec<T>,
    opt: SequenceOptions,
) -> Vec<Actions<T>> {
    // list of api requests ie. adds and deletes (return value)
    let mut actions = Vec::new();

    // Diffs are hard. We know the index of things in the new struct, but not in the steps inbetween.
    // Deletes first, then recompute diff, then Adds.
    // Keep in mind we do not have the ability specify snapshot for Adds
    let mut simulated = before.clone();

    // First pass: Deletions
    let h = IdentifyDistinct::<u32>::new(&before, 0..before.len(), &after, 0..after.len());
    let diff = capture_diff(
        similar::Algorithm::Lcs,
        h.old_lookup(),
        h.old_range(),
        h.new_lookup(),
        h.new_range(),
    );

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
                    .chunks(opt.max_n)
                    .into_iter()
                    .map(|chunk| match opt.delete_positional {
                        true => {
                            // delete by id and index
                            let chunk = chunk.map(|(i, t)| (i, t.clone())).collect_vec();
                            Actions::Delete(chunk)
                        }
                        false => {
                            // delete by id (all occurances)
                            let chunk = chunk.map(|(_, t)| t.clone()).collect_vec();
                            Actions::DeleteAll(chunk)
                        }
                    })
                    .collect_vec();
                actions.extend(a);

                // update simulation
                if opt.delete_positional {
                    let removed = simulated.drain(index.clone()).collect_vec();
                    assert!(removed == before[index], "diffs need to be sorted");
                } else {
                    // remove *all occurances* of ids
                    simulated.retain(|t| !before[index.clone()].contains(t))
                }
            }
            _ => { /* skip inserts on first pass */ }
        }
    }

    // Second pass: Insertions
    let h = IdentifyDistinct::<u32>::new(&simulated, 0..simulated.len(), &after, 0..after.len());
    let diff = capture_diff(
        similar::Algorithm::Lcs,
        h.old_lookup(),
        h.old_range(),
        h.new_lookup(),
        h.new_range(),
    );
    for d in diff.iter().rev() {
        match d {
            similar::DiffOp::Insert {
                old_index,
                new_index,
                new_len,
                ..
            } => {
                let new_slice = *new_index..(new_index + new_len);
                let elements = after[new_slice.clone()].to_owned();
                actions.push(Actions::Add(elements.clone(), *old_index));

                // update simulation
                simulated.splice(old_index..old_index, elements);

                let old_slice = *old_index..(old_index + new_len);
                assert!(simulated[old_slice] == after[new_slice]);
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
    let worst_case = after.len() / opt.max_n;
    if actions.len() >= worst_case {
        full_replace(after, opt)
    } else {
        actions
    }
}

pub fn full_replace<T: Clone + Eq>(after: Vec<T>, opt: SequenceOptions) -> Vec<Actions<T>> {
    let mut actions = Vec::new();
    let mut simulated = vec![];

    let items = after.iter().chunks(opt.max_n);
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
    actions
}

pub type SnapshotKey = usize;

// Available operations:
/// add up to 100 ids (position for group NO SNAPSHOT)
/// move up to 100 contiguous ids

pub enum Actions<T> {
    /// add up to 100 ids to end of playlist
    Append(Vec<T>),

    /// insert up to 100 ids at index
    Add(Vec<T>, usize),

    /// delete up to 100 ids (undocumented positional disambiguation)
    Delete(Vec<(usize, T)>),

    /// delete up to 100 ids (deletes all occurances of uri)
    DeleteAll(Vec<T>),

    /// replace (clear) the whole playlist with up to 100 ids
    Replace(Vec<T>),
}

#[cfg(test)]
mod test {
    use itertools::Itertools;

    use crate::utils::diff::SequenceOptions;

    use super::sequence;

    #[test]
    /// minimal test of playlist update sequenceing logic
    fn test_delete_append() {
        let before = vec!["red", "blue", "green", "yellow"];
        let after = vec!["blue", "green", "yellow", "black"];
        sequence(before.clone(), after.clone(), Default::default());
        sequence(after, before, Default::default());
    }

    #[test]
    /// robust test of playlist update sequenceing logic
    /// simulates many additions, deletions of varying sizes, including duplicate ids
    fn test_big_playlist() {
        fn test(opts: SequenceOptions) {
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
            sequence(before.clone(), after.clone(), opts);

            // going backwards also makes a good test
            sequence(after, before, opts);
        }

        test(Default::default());
        test(SequenceOptions {
            delete_positional: false,
            ..Default::default()
        });
    }
}
