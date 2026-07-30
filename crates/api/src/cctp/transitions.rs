//! Allowed CCTP transfer saga state transitions (frozen contract enum).

use crate::models::v2_cctp::CctpTransferStatus;

/// Returns true when `from` may transition to `to` (excluding idempotent same-state).
pub fn is_allowed_transition(from: CctpTransferStatus, to: CctpTransferStatus) -> bool {
    if from == to {
        return true; // idempotent duplicate events
    }

    use CctpTransferStatus::*;
    matches!(
        (from, to),
        (Created, BurnPrepared)
            | (Created, Cancelled)
            | (Created, ProviderKilled)
            | (BurnPrepared, BurnSubmitted)
            | (BurnPrepared, Cancelled)
            | (BurnPrepared, ProviderKilled)
            | (BurnSubmitted, AwaitingAttestation)
            | (BurnSubmitted, ProviderKilled)
            | (AwaitingAttestation, AttestationReady)
            | (AwaitingAttestation, AttestationFailed)
            | (AwaitingAttestation, ProviderKilled)
            | (AttestationFailed, AwaitingAttestation) // reattest re-poll only
            | (AttestationReady, MintPrepared)
            | (AttestationReady, ProviderKilled)
            | (MintPrepared, MintSubmitted)
            | (MintPrepared, MintFailedRetryable)
            | (MintPrepared, ProviderKilled)
            | (MintSubmitted, Completed)
            | (MintSubmitted, MintFailedRetryable)
            | (MintSubmitted, ProviderKilled)
            | (MintFailedRetryable, MintPrepared) // mint retry, same attestation
            | (MintFailedRetryable, ProviderKilled)
    )
}

/// Terminal states do not accept further transitions (except idempotent self).
pub fn is_terminal(status: CctpTransferStatus) -> bool {
    matches!(
        status,
        CctpTransferStatus::Completed
            | CctpTransferStatus::Cancelled
            | CctpTransferStatus::ProviderKilled
            | CctpTransferStatus::AttestationFailed
    )
}

/// Cancellation allowed only before source burn is submitted.
pub fn can_cancel(status: CctpTransferStatus) -> bool {
    matches!(
        status,
        CctpTransferStatus::Created | CctpTransferStatus::BurnPrepared
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::v2_cctp::CctpTransferStatus as S;

    #[test]
    fn every_valid_transition_allowed() {
        let pairs = [
            (S::Created, S::BurnPrepared),
            (S::Created, S::Cancelled),
            (S::BurnPrepared, S::BurnSubmitted),
            (S::BurnSubmitted, S::AwaitingAttestation),
            (S::AwaitingAttestation, S::AttestationReady),
            (S::AttestationReady, S::MintPrepared),
            (S::MintPrepared, S::MintSubmitted),
            (S::MintSubmitted, S::Completed),
            (S::MintSubmitted, S::MintFailedRetryable),
            (S::MintFailedRetryable, S::MintPrepared),
            (S::AttestationFailed, S::AwaitingAttestation),
        ];
        for (from, to) in pairs {
            assert!(is_allowed_transition(from, to), "{from:?} -> {to:?}");
        }
    }

    #[test]
    fn reburn_and_early_states_blocked_after_burn_submitted() {
        assert!(!is_allowed_transition(S::BurnSubmitted, S::Created));
        assert!(!is_allowed_transition(S::BurnSubmitted, S::BurnPrepared));
        assert!(!is_allowed_transition(S::AwaitingAttestation, S::Created));
        assert!(!is_allowed_transition(
            S::AwaitingAttestation,
            S::BurnPrepared
        ));
        assert!(!is_allowed_transition(
            S::AttestationReady,
            S::BurnSubmitted
        ));
        assert!(!is_allowed_transition(S::Completed, S::MintPrepared));
    }

    #[test]
    fn cancellation_only_before_burn_submitted() {
        assert!(can_cancel(S::Created));
        assert!(can_cancel(S::BurnPrepared));
        assert!(!can_cancel(S::BurnSubmitted));
        assert!(!can_cancel(S::AwaitingAttestation));
    }

    #[test]
    fn idempotent_same_state() {
        assert!(is_allowed_transition(
            S::AwaitingAttestation,
            S::AwaitingAttestation
        ));
    }
}
