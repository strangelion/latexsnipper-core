#![no_main]

use latexsnipper_plugin::{
    canonical_signed_envelope_bytes, decode_signed_metadata, verify_initial_root, RootMetadata,
    SignedMetadata, SnapshotMetadata, TargetsMetadata, TimestampMetadata,
};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Some((&kind, payload)) = data.split_first() else {
        return;
    };
    match kind % 4 {
        0 => {
            if let Ok(value) = decode_signed_metadata::<RootMetadata>(payload) {
                let _ = canonical_signed_envelope_bytes(&value);
                let _ = verify_initial_root(&value, 1_900_000_000);
            }
        }
        1 => canonicalize::<TimestampMetadata>(payload),
        2 => canonicalize::<SnapshotMetadata>(payload),
        _ => canonicalize::<TargetsMetadata>(payload),
    }
});

fn canonicalize<T: serde::Serialize + serde::de::DeserializeOwned>(data: &[u8]) {
    if let Ok(value) = decode_signed_metadata::<T>(data) {
        let envelope: SignedMetadata<T> = value;
        let _ = canonical_signed_envelope_bytes(&envelope);
    }
}
