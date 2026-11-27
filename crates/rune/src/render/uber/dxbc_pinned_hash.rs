// Written by `cargo run -p rune --example regen_dxbc` on Windows with uber.fs-k0/k1/k2.dxbc; nothing else writes them.
/// BLAKE3 of the WGSL each pinned fragment was compiled from (render/variant.rs `pinned_wgsl`, `PINNED` order).
pub(super) const UBER_DXBC_PINNED_WGSL_BLAKE3: [&str; 3] = ["64ccdd935c32f5bc90d815bb5b8fd7d3b0edf67f7038f7f7fc2feca30f59dd51", "ba9f67f14710170dffac7be069a0b0e64af960a0442957cfa1286b868c79f1d3", "8de146b238689c7c017eb91ab4d836f2fa473c07e8d68916012aac3c1ed98e40"];
/// BLAKE3 of each pinned fragment's file, and of the `uber.vs.dxbc` it was compiled beside.
pub(super) const UBER_DXBC_PINNED_FS_BLAKE3: [&str; 3] = ["c36098265655eb235780db672c0dd61ddbe0cb24e8e980157a11c2d0253d8b32", "373bbf31ceaaab776daec4f86934850633e3c84dc76f650cb9afba72756cfcd6", "206a8411f8fc76a2c247a08d8f8f6a3f73c07d666dedf73b7ca8b59eb2b8104e"];
pub(super) const UBER_DXBC_PINNED_VS_BLAKE3: &str = "e6a25a34357594d88ffdbe2f8f6e242ee9b42aa46005cc71d067b4891bbdd42c";
