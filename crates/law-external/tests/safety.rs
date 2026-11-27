mod common;

use law_external::Applicability;
use law_model::{FixPolicy, Safety};

fn none() -> FixPolicy {
    FixPolicy::None {
        reason: "ported".to_owned(),
    }
}

#[test]
fn safety_is_the_weaker_of_ruff_applicability_and_the_rule_policy_all_pairs() {
    let table = [
        (Applicability::Safe, FixPolicy::Safe, Some(Safety::Safe)),
        (Applicability::Safe, FixPolicy::Unsafe, Some(Safety::Unsafe)),
        (Applicability::Safe, none(), None),
        (Applicability::Unsafe, FixPolicy::Safe, Some(Safety::Unsafe)),
        (Applicability::Unsafe, FixPolicy::Unsafe, Some(Safety::Unsafe)),
        (Applicability::Unsafe, none(), None),
        (Applicability::DisplayOnly, FixPolicy::Safe, None),
        (Applicability::DisplayOnly, FixPolicy::Unsafe, None),
        (Applicability::DisplayOnly, none(), None),
    ];
    for (ruff, policy, expected) in &table {
        assert_eq!(ruff.safety(policy), *expected, "{ruff:?} under {policy:?}");
    }
}
