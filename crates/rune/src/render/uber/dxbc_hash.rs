// Written by `cargo run -p rune --example regen_dxbc` on Windows, the one way to regenerate the uber bytecode:
// it also writes the pinned fragments and their hashes (dxbc_pinned_hash.rs), which nothing else updates.
pub(super) const UBER_DXBC_WGSL_BLAKE3: &str = "117330c3ebd75454da6a17913b8b0539ba751bb51afba9f08b329c2b7aae4f04";
pub(super) const UBER_DXBC_ABI_FNV64: u64 = 0x6b9932e7795ec011;
#[cfg(test)]
pub(super) const UBER_DXBC_HLSL_BLAKE3: &str = "7254839e35cb36f5d43bc7640cd3fc6d3c6562a8719c77202f85f2586bca76ba";
#[cfg(test)]
pub(super) const UBER_DXBC_DUAL_HLSL_BLAKE3: &str = "57afb8def0d5f8eae4016aa6038ac14967c92c729ee919dd9bd5cd295dd4454b";
