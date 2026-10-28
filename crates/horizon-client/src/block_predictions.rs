//! `BlockStatePredictionHandler`: the blocks this client changed ahead of the server (a block
//! broken, placed or opened), kept until the server acknowledges the action that changed them.
use std::collections::HashMap;

use horizon_core::geometry::Vec3;

/// A predicted block as the server last described it (`ServerVerifiedState`).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Verified {
    /// The last action that changed the block.
    sequence: i32,
    /// The state the server last said the block has.
    state: i32,
    /// Where the player stood when the block was first predicted.
    player: Option<Vec3>,
}

/// A block an acknowledgement settled: what `ClientLevel.syncBlockState` is given.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settled {
    pub pos: [i32; 3],
    /// The server's state, which the block takes.
    pub state: i32,
    /// Where the player goes back to if that state is solid where the player is now. None
    /// when the server teleported the player after the action: that position stands.
    pub player: Option<Vec3>,
}

/// The sequence numbers of the actions this client predicts, and the blocks they changed.
#[derive(Debug)]
pub struct BlockPredictions {
    verified: HashMap<[i32; 3], Verified>,
    sequence: i32,
    last_teleport: i32,
}

impl Default for BlockPredictions {
    fn default() -> Self {
        Self {
            verified: HashMap::new(),
            sequence: 0,
            last_teleport: -1,
        }
    }
}

impl BlockPredictions {
    /// `startPredicting`: the sequence of a new action, which its packet carries.
    pub fn start(&mut self) -> i32 {
        self.sequence += 1;
        self.sequence
    }
    /// `retainKnownServerState`: a block the action `sequence` changed. Its first prediction
    /// keeps the state it had and where the player stood; a later one only moves it to the
    /// later action.
    pub fn retain(&mut self, pos: [i32; 3], state: i32, player: Option<Vec3>, sequence: i32) {
        self.verified
            .entry(pos)
            .and_modify(|v| v.sequence = sequence)
            .or_insert(Verified {
                sequence,
                state,
                player,
            });
    }
    /// `updateKnownServerState`: the server's update of a predicted block waits for the
    /// acknowledgement. False when the block is not predicted: it changes at once.
    pub fn update(&mut self, pos: [i32; 3], state: i32) -> bool {
        match self.verified.get_mut(&pos) {
            Some(v) => {
                v.state = state;
                true
            }
            None => false,
        }
    }
    /// `endPredictionsUpTo`: the blocks of the actions up to `sequence`, which now take the
    /// server's state.
    pub fn end_up_to(&mut self, sequence: i32) -> Vec<Settled> {
        let back = self.last_teleport < sequence;
        let mut settled = Vec::new();
        self.verified.retain(|&pos, v| {
            if v.sequence > sequence {
                return true;
            }
            settled.push(Settled {
                pos,
                state: v.state,
                player: v.player.filter(|_| back),
            });
            false
        });
        settled
    }
    /// `onTeleport`: the server placed the player, so the actions sent so far no longer put
    /// the player back when they settle.
    pub fn on_teleport(&mut self) {
        self.last_teleport = self.sequence;
    }
    /// A new level (a respawn): none of its blocks are predicted.
    pub fn clear(&mut self) {
        self.verified.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STONE: i32 = 1;
    const AIR: i32 = 0;

    #[test]
    fn a_break_the_server_confirms_settles_on_the_ack_and_not_before() {
        let mut p = BlockPredictions::default();
        let at = Vec3::new(0.5, 60.0, 0.5);
        let seq = p.start();
        p.retain([0, 62, 0], STONE, Some(at), seq);
        // The server's update for the block waits; an older ack settles nothing.
        assert!(p.update([0, 62, 0], AIR));
        assert!(!p.update([0, 63, 0], AIR));
        assert!(p.end_up_to(seq - 1).is_empty());
        assert_eq!(
            p.end_up_to(seq),
            [Settled {
                pos: [0, 62, 0],
                state: AIR,
                player: Some(at),
            }]
        );
        assert!(p.end_up_to(seq).is_empty());
    }

    #[test]
    fn a_refused_break_comes_back_with_where_the_player_stood_when_it_was_broken() {
        let mut p = BlockPredictions::default();
        let first = Vec3::new(0.5, 60.0, 0.5);
        let a = p.start();
        p.retain([0, 62, 0], STONE, Some(first), a);
        // A later action changes the block again before the first is acknowledged (a block
        // placed back there): the first state and position stay, and it waits for the later ack.
        let b = p.start();
        p.retain([0, 62, 0], AIR, Some(Vec3::new(0.5, 60.4, 0.5)), b);
        assert!(p.end_up_to(a).is_empty());
        assert_eq!(
            p.end_up_to(b),
            [Settled {
                pos: [0, 62, 0],
                state: STONE,
                player: Some(first),
            }]
        );
    }

    #[test]
    fn a_teleport_after_the_action_keeps_the_player_where_the_server_put_it() {
        let mut p = BlockPredictions::default();
        let a = p.start();
        p.retain([0, 62, 0], STONE, Some(Vec3::new(0.5, 60.0, 0.5)), a);
        p.on_teleport();
        let settled = p.end_up_to(a);
        assert_eq!(settled.len(), 1);
        assert_eq!(settled[0].player, None);
        // An action after the teleport puts the player back again.
        let b = p.start();
        p.retain([0, 63, 0], STONE, Some(Vec3::new(0.5, 61.0, 0.5)), b);
        assert!(p.end_up_to(b)[0].player.is_some());
    }

    #[test]
    fn a_respawn_forgets_the_blocks_but_keeps_counting() {
        let mut p = BlockPredictions::default();
        let a = p.start();
        p.retain([0, 62, 0], STONE, None, a);
        p.clear();
        assert!(p.end_up_to(a).is_empty());
        assert_eq!(p.start(), a + 1);
    }
}
