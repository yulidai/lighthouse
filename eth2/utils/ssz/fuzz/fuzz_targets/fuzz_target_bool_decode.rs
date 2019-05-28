#![no_main]
#[macro_use] extern crate libfuzzer_sys;
extern crate ssz;

use ssz::{DecodeError, Decode};

// Fuzz ssz_decode()
fuzz_target!(|data: &[u8]| {
    let result: Result<bool, DecodeError> = bool::from_ssz_bytes(data);
    if data.len() == 1 {
        if data[0] == 1 {
            let val_bool = result.unwrap();
            assert!(val_bool);
        } else if data[0] == 0 {
            let val_bool = result.unwrap();
            assert!(!val_bool);
        } else {
            assert!(result.is_err());
        }
    } else {
        // Length of 0 should return error
        assert!(result.is_err());
    }
});
