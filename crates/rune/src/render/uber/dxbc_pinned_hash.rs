// Written by `cargo run -p rune --example regen_dxbc` on Windows with uber.fs-k0/k1/k2.dxbc; nothing else writes them.
/// BLAKE3 of the WGSL each pinned fragment was compiled from (render/variant.rs `pinned_wgsl`, `PINNED` order).
pub(super) const UBER_DXBC_PINNED_WGSL_BLAKE3: [&str; 3] = ["4351c8dd278bb30aaf63a31f199ba40d231fe2366c6f1fa4f07c6f50de7a0a5f", "bdb12393a5746353cf1d7e35324924ccfe4cab5282a77f41e0f9d1bcb50732a7", "aa46db92c657b157aa89fdeb065087a876876e526e91d9e2c22dde7d48949ebc"];
/// BLAKE3 of each pinned fragment's file, and of the `uber.vs.dxbc` it was compiled beside.
pub(super) const UBER_DXBC_PINNED_FS_BLAKE3: [&str; 3] = ["c36098265655eb235780db672c0dd61ddbe0cb24e8e980157a11c2d0253d8b32", "373bbf31ceaaab776daec4f86934850633e3c84dc76f650cb9afba72756cfcd6", "206a8411f8fc76a2c247a08d8f8f6a3f73c07d666dedf73b7ca8b59eb2b8104e"];
pub(super) const UBER_DXBC_PINNED_VS_BLAKE3: &str = "e6a25a34357594d88ffdbe2f8f6e242ee9b42aa46005cc71d067b4891bbdd42c";
