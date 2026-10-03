#![no_main]

use libfuzzer_sys::fuzz_target;
use newton_manifest::{
    DecodeLimits, from_bytes_with_limits, raw_from_bytes_borrowed_with_limits, raw_to_bytes,
};

fuzz_target!(|data: &[u8]| {
    let limits = DecodeLimits {
        max_groups: 10_000,
        max_subgroups_per_group: 10_000,
        max_resources_per_group: 100_000,
        max_total_entries: 100_000,
        max_string_bytes: 1024 * 1024,
        max_total_string_bytes: 8 * 1024 * 1024,
        max_allocation_bytes: 32 * 1024 * 1024,
    };

    let _ = from_bytes_with_limits(data, limits);
    if let Ok(raw) = raw_from_bytes_borrowed_with_limits(data, limits) {
        let encoded = raw_to_bytes(&raw).expect("decoded raw NEWTON should re-encode");
        assert_eq!(encoded, data);
    }
});
