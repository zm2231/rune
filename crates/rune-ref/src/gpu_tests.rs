use crate::gpu::{DEFAULT_TOLERANCE, composite_tolerance};

fn adapter(backend: wgpu::Backend, vendor: u32, name: &str) -> wgpu::AdapterInfo {
    wgpu::AdapterInfo {
        name: name.into(),
        vendor,
        ..wgpu::AdapterInfo::new(wgpu::DeviceType::DiscreteGpu, backend)
    }
}

fn tolerance(backend: wgpu::Backend, vendor: u32, name: &str, draws: usize) -> u8 {
    composite_tolerance(&adapter(backend, vendor, name)).for_draws(draws)
}

#[test]
fn composite_tolerance_is_three_for_nvidia_dx12_and_two_on_other_measured_adapters() {
    for draws in [2, 8] {
        assert_eq!(tolerance(wgpu::Backend::Dx12, 0x10DE, "NVIDIA GeForce", draws), 3);
        assert_eq!(
            tolerance(wgpu::Backend::Vulkan, 0x10DE, "NVIDIA GeForce", draws),
            DEFAULT_TOLERANCE,
            "NVIDIA under Vulkan is not measured and stays at 2"
        );
        assert_eq!(tolerance(wgpu::Backend::Gl, 0x10DE, "NVIDIA GeForce", draws), DEFAULT_TOLERANCE);
        assert_eq!(tolerance(wgpu::Backend::Dx12, 0x1002, "AMD Radeon Graphics", draws), DEFAULT_TOLERANCE);
        assert_eq!(
            tolerance(wgpu::Backend::Dx12, 0x1414, "Microsoft Basic Render Driver", draws),
            DEFAULT_TOLERANCE
        );
        assert_eq!(tolerance(wgpu::Backend::Dx12, 0x8086, "Intel Iris Xe", draws), DEFAULT_TOLERANCE);
        assert_eq!(tolerance(wgpu::Backend::Metal, 0x106B, "Apple M4 Pro", draws), DEFAULT_TOLERANCE);
    }
}

#[test]
fn lavapipe_composite_tolerance_grows_one_per_draw() {
    let lavapipe = |draws| tolerance(wgpu::Backend::Vulkan, 0x10005, "llvmpipe (LLVM 20.1.2, 256 bits)", draws);
    assert_eq!(lavapipe(2), 3);
    assert_eq!(lavapipe(4), 5);
    assert_eq!(lavapipe(8), 9);
    assert_eq!(lavapipe(10_000), u8::MAX);
    assert_eq!(tolerance(wgpu::Backend::Gl, 0x10005, "llvmpipe", 4), DEFAULT_TOLERANCE, "only lavapipe's Vulkan is measured");
}

#[test]
fn try_new_returns_error_without_panicking_when_no_gpu() {
    match crate::gpu::GpuRenderer::try_new() {
        Ok(gpu) => assert!(!gpu.adapter_info().name.is_empty()),
        Err(err) => assert!(err.starts_with("no GPU adapter:"), "unexpected error: {err}"),
    }
}
