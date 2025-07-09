use super::constants::*;
use super::vote::VoteAttestation;
use alloy_consensus::Header;
use alloy_rlp as rlp;

/// Extract the `VoteAttestation` bytes slice from `header.extra_data` if present and decode.
///
/// * `epoch_len` – current epoch length (200/500/1000) so we can determine if block is an epoch boundary.
/// * `is_luban` – true once Luban hard-fork active (extraData format changes).
/// * `is_bohr`  – true once Bohr hard-fork active (turnLength byte present).
pub fn parse_vote_attestation_from_header(
    header: &Header,
    epoch_len: u64,
    is_luban: bool,
    is_bohr: bool,
) -> Option<VoteAttestation> {
    let extra = header.extra_data.as_ref();
    if extra.len() <= EXTRA_VANITY + EXTRA_SEAL {
        return None;
    }
    if !is_luban {
        return None; // attestation introduced in Luban
    }

    // Determine attestation slice boundaries.
    let number = header.number;

    let att_bytes = if number % epoch_len == 0 {
        // Epoch block (contains validator bytes + optional turnLength)
        let num_validators = extra[EXTRA_VANITY] as usize; // first byte after vanity
        let mut start = EXTRA_VANITY + VALIDATOR_NUMBER_SIZE + num_validators * VALIDATOR_BYTES_LEN_AFTER_LUBAN;
        if is_bohr {
            start += TURN_LENGTH_SIZE;
        }
        let end = extra.len() - EXTRA_SEAL;
        if end <= start {
            return None;
        }
        &extra[start..end]
    } else {
        // Normal block: attestation directly after vanity
        let start = EXTRA_VANITY;
        let end = extra.len() - EXTRA_SEAL;
        &extra[start..end]
    };

    if att_bytes.is_empty() {
        return None;
    }

    match VoteAttestation::decode_rlp(att_bytes) {
        Ok(a) => Some(a),
        Err(_) => None,
    }
} 