use tree_hash::TreeHash;
use types::test_utils::TestingBeaconStateBuilder;
use types::{BeaconState, EthSpec, Hash256, MainnetEthSpec};

const LOOPS: usize = 5_000;
const LEN: usize = 8_192;

fn build_state<T: EthSpec>(validator_count: usize) -> BeaconState<T> {
    let (state, _keypairs) = TestingBeaconStateBuilder::from_default_keypairs_file_if_exists(
        validator_count,
        &T::default_spec(),
    )
    .build();

    assert_eq!(state.validators.len(), validator_count);
    assert_eq!(state.balances.len(), validator_count);
    assert!(state.previous_epoch_attestations.is_empty());
    assert!(state.current_epoch_attestations.is_empty());
    assert!(state.eth1_data_votes.is_empty());
    assert!(state.historical_roots.is_empty());

    state
}

fn main() {
    let mut vec = Vec::with_capacity(LEN);

    for _ in 0..LEN {
        vec.push(Hash256::random());
    }

    for _ in 0..LOOPS {
        let tree_height = Hash256::tree_hash_packing().height_for_value_count(LEN);
        let mut hasher = tree_hash::VecTreeHasher::packed(tree_height);

        vec.iter().for_each(|item| {
            item.tree_hash_apply_root(|bytes| {
                hasher.update(bytes);
            })
        });

        let x = hasher.finish();
    }
}
