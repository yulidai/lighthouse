use super::*;
use ethereum_types::H256;

macro_rules! impl_for_bitsize {
    ($type: ident, $bit_size: expr) => {
        impl TreeHash for $type {
            fn tree_hash_apply_root<F>(&self, mut f: F)
            where
                F: FnMut(&[u8]),
            {
                f(&self.to_le_bytes())
            }

            fn tree_hash_packing() -> TreeHashPacking {
                // FIXME: assumes $bit_size <= 256.
                TreeHashPacking::Packed {
                    packing_factor: HASHSIZE / ($bit_size / 8),
                }
            }

            fn tree_hash_root(&self) -> Vec<u8> {
                let mut bytes = self.to_le_bytes().to_vec();
                // FIXME: assumes $bit_size <= 256.
                bytes.resize(BYTES_PER_CHUNK, 0);
                bytes
            }
        }
    };
}

impl_for_bitsize!(u8, 8);
impl_for_bitsize!(u16, 16);
impl_for_bitsize!(u32, 32);
impl_for_bitsize!(u64, 64);
impl_for_bitsize!(usize, 64);

impl TreeHash for bool {
    fn tree_hash_apply_root<F>(&self, f: F)
    where
        F: FnMut(&[u8]),
    {
        (*self as u8).tree_hash_apply_root(f)
    }

    fn tree_hash_packing() -> TreeHashPacking {
        u8::tree_hash_packing()
    }

    fn tree_hash_root(&self) -> Vec<u8> {
        (*self as u8).tree_hash_root()
    }
}

macro_rules! impl_for_u8_array {
    ($len: expr) => {
        impl TreeHash for [u8; $len] {
            fn tree_hash_apply_root<F>(&self, mut f: F)
            where
                F: FnMut(&[u8]),
            {
                f(&self[..])
            }

            fn tree_hash_packing() -> TreeHashPacking {
                TreeHashPacking::NotPacked
            }

            fn tree_hash_root(&self) -> Vec<u8> {
                let mut bytes = self.to_vec();
                // FIXME: assumes $bit_size <= 256.
                bytes.resize(BYTES_PER_CHUNK, 0);
                bytes
            }
        }
    };
}

impl_for_u8_array!(4);
impl_for_u8_array!(32);

impl TreeHash for H256 {
    fn tree_hash_apply_root<F>(&self, mut f: F)
    where
        F: FnMut(&[u8]),
    {
        f(&self[..])
    }

    fn tree_hash_packing() -> TreeHashPacking {
        TreeHashPacking::NotPacked
    }

    fn tree_hash_root(&self) -> Vec<u8> {
        self.as_bytes().to_vec()
    }
}

// TODO: this implementation always panics, it only exists to allow us to compile whilst
// refactoring tree hash. Should be removed.
macro_rules! impl_for_list {
    ($type: ty) => {
        impl<T> TreeHash for $type
        where
            T: TreeHash,
        {
            fn tree_hash_apply_root<F>(&self, _f: F)
            where
                F: FnMut(&[u8]),
            {
                unimplemented!("TreeHash is not implemented for Vec or slice")
            }

            fn tree_hash_packing() -> TreeHashPacking {
                TreeHashPacking::NotPacked
            }

            fn tree_hash_root(&self) -> Vec<u8> {
                unimplemented!("TreeHash is not implemented for Vec or slice")
            }
        }
    };
}

impl_for_list!(Vec<T>);
impl_for_list!(&[T]);

#[cfg(test)]
mod test {
    use super::*;

    /// Returns `int` as little-endian bytes with a length of 32.
    fn int_to_bytes32(int: u64) -> Vec<u8> {
        let mut vec = int.to_le_bytes().to_vec();
        vec.resize(32, 0);
        vec
    }

    #[test]
    fn bool() {
        let mut true_bytes: Vec<u8> = vec![1];
        true_bytes.append(&mut vec![0; 31]);

        let false_bytes: Vec<u8> = vec![0; 32];

        assert_eq!(true.tree_hash_root(), true_bytes);
        assert_eq!(false.tree_hash_root(), false_bytes);
    }

    #[test]
    fn int_to_bytes() {
        assert_eq!(&int_to_bytes32(0), &[0; 32]);
        assert_eq!(
            &int_to_bytes32(1),
            &[
                1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0
            ]
        );
        assert_eq!(
            &int_to_bytes32(u64::max_value()),
            &[
                255, 255, 255, 255, 255, 255, 255, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ]
        );
    }
}
