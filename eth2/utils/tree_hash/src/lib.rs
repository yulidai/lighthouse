#[macro_use]
extern crate lazy_static;

pub mod impls;
pub mod iterator_hasher;
mod merkleize_padded;
mod merkleize_standard;

pub use iterator_hasher::VecTreeHasher;
pub use merkleize_padded::merkleize_padded;
pub use merkleize_standard::merkleize_standard;

pub const BYTES_PER_CHUNK: usize = 32;
pub const HASHSIZE: usize = 32;
pub const MERKLE_HASH_CHUNK: usize = 2 * BYTES_PER_CHUNK;

/// Alias to `merkleize_padded(&bytes, minimum_chunk_count)`
///
/// If `minimum_chunk_count < bytes / BYTES_PER_CHUNK`, padding will be added for the difference
/// between the two.
pub fn merkle_root(bytes: &[u8], minimum_chunk_count: usize) -> Vec<u8> {
    merkleize_padded(&bytes, minimum_chunk_count)
}

/// Returns the node created by hashing `root` and `length`.
///
/// Used in `TreeHash` for inserting the length of a list above it's root.
pub fn mix_in_length(root: &[u8], length: usize) -> Vec<u8> {
    let mut length_bytes = length.to_le_bytes().to_vec();
    length_bytes.resize(BYTES_PER_CHUNK, 0);

    merkleize_padded::hash_concat(root, &length_bytes)
}

pub fn height_for_leaf_count(leaf_count: usize) -> usize {
    leaf_count.next_power_of_two().trailing_zeros() as usize + 1
}

#[derive(Debug)]
pub enum TreeHashPacking {
    NotPacked,
    Packed { packing_factor: usize },
}

impl TreeHashPacking {
    pub fn height_for_value_count(&self, value_count: usize) -> usize {
        let num_leaves = match self {
            TreeHashPacking::NotPacked => value_count,
            TreeHashPacking::Packed { packing_factor } => {
                (value_count + packing_factor - 1) / packing_factor
            }
        };

        height_for_leaf_count(num_leaves)
    }
}

pub trait TreeHash {
    fn tree_hash_apply_root<F>(&self, f: F)
    where
        F: FnMut(&[u8]);

    fn tree_hash_packing() -> TreeHashPacking;

    fn tree_hash_root(&self) -> Vec<u8>;
}

pub trait SignedRoot: TreeHash {
    fn signed_root(&self) -> Vec<u8>;
}

/*
#[macro_export]
macro_rules! tree_hash_ssz_encoding_as_vector {
    ($type: ident) => {
        impl tree_hash::TreeHash for $type {
            fn tree_hash_type() -> tree_hash::TreeHashType {
                tree_hash::TreeHashType::Vector
            }

            fn tree_hash_packed_encoding(&self) -> Vec<u8> {
                unreachable!("Vector should never be packed.")
            }

            fn tree_hash_packing_factor() -> usize {
                unreachable!("Vector should never be packed.")
            }

            fn tree_hash_root(&self) -> Vec<u8> {
                tree_hash::merkle_root(&ssz::ssz_encode(self))
            }
        }
    };
}

#[macro_export]
macro_rules! tree_hash_ssz_encoding_as_list {
    ($type: ident) => {
        impl tree_hash::TreeHash for $type {
            fn tree_hash_type() -> tree_hash::TreeHashType {
                tree_hash::TreeHashType::List
            }

            fn tree_hash_packed_encoding(&self) -> Vec<u8> {
                unreachable!("List should never be packed.")
            }

            fn tree_hash_packing_factor() -> usize {
                unreachable!("List should never be packed.")
            }

            fn tree_hash_root(&self) -> Vec<u8> {
                ssz::ssz_encode(self).tree_hash_root()
            }
        }
    };
}
*/

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn mix_length() {
        let hash = {
            let mut preimage = vec![42; BYTES_PER_CHUNK];
            preimage.append(&mut vec![42]);
            preimage.append(&mut vec![0; BYTES_PER_CHUNK - 1]);
            eth2_hashing::hash(&preimage)
        };

        assert_eq!(mix_in_length(&[42; BYTES_PER_CHUNK], 42), hash);
    }
}
