//! Shared top-level container invariants.

use crate::error::{BnkError, Result};
use crate::types::ChunkId;
use crate::version::BankVersion;

pub(crate) fn validate_twinning_chunk_ids(
    version: BankVersion,
    ids: impl IntoIterator<Item = ChunkId>,
) -> Result<()> {
    let mut previous_rank = None;
    let mut previous_id = None;
    let mut has_setting = false;
    let mut has_game_synchronization = false;
    let mut has_init = false;
    let mut has_platform = false;

    for id in ids {
        let rank = match id {
            ChunkId::DIDX => 0,
            ChunkId::DATA => {
                if previous_id != Some(ChunkId::DIDX) {
                    return Err(BnkError::invalid(
                        "Twinning chunk sequence",
                        0,
                        "DATA must be immediately preceded by DIDX",
                    ));
                }
                1
            }
            ChunkId::INIT => {
                has_setting = true;
                has_init = true;
                2
            }
            ChunkId::STMG => {
                has_setting = true;
                has_game_synchronization = true;
                3
            }
            ChunkId::HIRC => 4,
            ChunkId::STID => 5,
            ChunkId::ENVS => {
                has_setting = true;
                6
            }
            ChunkId::PLAT => {
                has_setting = true;
                has_platform = true;
                7
            }
            id => {
                return Err(BnkError::invalid(
                    "Twinning chunk sequence",
                    0,
                    format!("unknown chunk {id:?} is not part of Twinning's model"),
                ));
            }
        };
        if previous_rank.is_some_and(|previous| rank <= previous) {
            return Err(BnkError::invalid(
                "Twinning chunk sequence",
                0,
                format!("chunk {id:?} is duplicated or appears outside canonical order"),
            ));
        }
        previous_rank = Some(rank);
        previous_id = Some(id);
    }

    if has_setting != has_game_synchronization {
        return Err(BnkError::invalid(
            "Twinning chunk sequence",
            0,
            "INIT/ENVS/PLAT settings and STMG game synchronization must occur together",
        ));
    }
    if version.before(118) && has_init {
        return Err(BnkError::invalid(
            "Twinning chunk sequence",
            0,
            format!("INIT is not defined for Wwise {}", version.number()),
        ));
    }
    if version.before(113) && has_platform {
        return Err(BnkError::invalid(
            "Twinning chunk sequence",
            0,
            format!("PLAT is not defined for Wwise {}", version.number()),
        ));
    }
    Ok(())
}
