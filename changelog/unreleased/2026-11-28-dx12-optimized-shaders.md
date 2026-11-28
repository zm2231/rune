- **Debug builds on Windows compile optimized shaders, as release builds do.** wgpu's debug-build
  default compiled every FXC shader unoptimized, which made WARP's shader compiler and rasterizer
  many times slower on the larger effect shaders and had debug builds and tests running different
  shaders from release. `WGPU_DEBUG=1` brings back unoptimized, debuggable shaders, and
  `WGPU_VALIDATION=0` turns off the D3D12 debug layer that debug builds load.
