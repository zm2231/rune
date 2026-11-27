// Written by `cargo run -p rune --example regen_dxbc` on Windows, the one way to regenerate the uber bytecode:
// it also writes the pinned fragments and their hashes (dxbc_pinned_hash.rs), which nothing else updates.
pub(super) const UBER_DXBC_WGSL_BLAKE3: &str = "ae1c4caf534d1efd0e3730cf4ee3651aa40b395f51c10d6c64aea75c05453883";
pub(super) const UBER_DXBC_ABI_FNV64: u64 = 0x6b9932e7795ec011;
#[cfg(test)]
pub(super) const UBER_DXBC_HLSL_BLAKE3: &str = "6e7a9ed3749b48d3708c63a1be8d4eb8452ae1228ea7730d60ef1812303583a7";
#[cfg(test)]
pub(super) const UBER_DXBC_DUAL_HLSL_BLAKE3: &str = "7a1a5b8697de754b600e5b5f248c18220c4b6862839493afa8e83aa0a03c242d";
