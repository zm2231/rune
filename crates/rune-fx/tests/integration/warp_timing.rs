
use std::time::{Duration, Instant};

use rune::render::Gpu;
use rune_fx::*;

const SIZE: u32 = 64;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

fn rows() -> Vec<(String, Fx)> {
    let mut rows: Vec<(String, Fx)> = vec![
        ("glow".into(), Fx::glow(GlowParams::default())),
        ("pulse".into(), Fx::pulse(PulseParams::default())),
        ("electric".into(), Fx::electric(ElectricParams::default())),
        ("fire".into(), Fx::fire(FireParams::default())),
        ("fire/heat".into(), Fx::fire(FireParams::heat())),
        ("fire/plasma".into(), Fx::fire(FireParams::plasma())),
        ("distort/heat_haze".into(), Fx::distort(DistortParams::HeatHaze { height: 40.0, scale: 1.0, speed: 1.0, amount: 4.0, whole_shape: true })),
        ("distort/shockwave".into(), Fx::distort(DistortParams::shockwave([32.0, 32.0], 0.0))),
        ("distort/crt".into(), Fx::distort(DistortParams::crt())),
        ("distort/chroma_split".into(), Fx::distort(DistortParams::chroma_split([2.0, 0.0]))),
        ("distort/lens".into(), Fx::lens(LensParams::default())),
        ("distort/lens_frost".into(), Fx::lens(LensParams { frost: 1.0, ..Default::default() })),
        ("motion_blur".into(), motion_blur()),
        ("extras/shimmer".into(), Fx::shimmer(ShimmerParams::default())),
        ("extras/sparkles".into(), Fx::sparkles(SparkleParams::default())),
        ("extras/aurora".into(), Fx::aurora(AuroraParams::default())),
        ("extras/grain".into(), Fx::grain(GrainParams::default())),
    ];
    rows.extend(fill_styles().iter().map(|s| (format!("fill/{}", s.id), Fx::fill(s.default_fill()))));
    rows
}

fn motion_blur() -> Fx {
    let rest = Rect::new(16.0, 16.0, 32.0, 32.0);
    let p = MotionBlurParams { mode: BlurMode::Shutter, shutter_angle: 180.0, ..Default::default() };
    let samples = MotionSamples::from_track(&p, rest, 0.1, 1.0, |t| Pose { x: (t - 0.1) * 600.0, y: 0.0, rotation: 0.0, scale: 1.0, opacity: 1.0 });
    Fx::motion_blur(p, samples)
}

fn texture(gpu: &Gpu, usage: wgpu::TextureUsages) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d { width: SIZE, height: SIZE, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    (texture, view)
}

fn clear(encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView, color: wgpu::Color) {
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations { load: wgpu::LoadOp::Clear(color), store: wgpu::StoreOp::Store },
        })],
        ..Default::default()
    });
}

struct Frame {
    took: Duration,
    drawn: u32,
    blurred: bool,
}

fn frame(gpu: &Gpu, fx: &mut FxRenderer, layer: &FxLayer, backdrop: &wgpu::TextureView, source: &wgpu::TextureView, target: &wgpu::TextureView) -> Result<Frame, String> {
    let started = Instant::now();
    let frame = FxFrame { viewport: [SIZE, SIZE], scale: 1.0, layers: std::slice::from_ref(layer), backdrop: None, sources: &[source] };
    let drawn = fx.prepare(&gpu.device, &gpu.queue, &frame).drawn;
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    let blurred = fx.blur_backdrop(&gpu.device, &gpu.queue, &mut encoder, backdrop, 0);
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
            })],
            ..Default::default()
        });
        fx.draw_layer(&mut pass, 0);
    }
    gpu.queue.submit([encoder.finish()]);
    gpu.device.poll(wgpu::PollType::wait_indefinitely()).map_err(|e| format!("{e:?}"))?;
    Ok(Frame { took: started.elapsed(), drawn, blurred })
}

#[test]
#[ignore]
fn warp_timing() {
    let Some(gpu) = Gpu::test_headless() else { return };
    let only = std::env::var("RUNE_FX_TIMING_ONLY").ok();
    let info = gpu.adapter.get_info();
    println!("adapter: {} ({:?}, {:?})", info.name, info.backend, info.device_type);
    println!("WGPU_DEBUG={:?} WGPU_VALIDATION={:?} debug_assertions={}", std::env::var("WGPU_DEBUG").ok(), std::env::var("WGPU_VALIDATION").ok(), cfg!(debug_assertions));
    let sampled = wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;
    let (_b, backdrop) = texture(&gpu, sampled);
    let (_s, source) = texture(&gpu, sampled);
    let (_t, target) = texture(&gpu, wgpu::TextureUsages::RENDER_ATTACHMENT);
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    clear(&mut encoder, &backdrop, wgpu::Color { r: 0.5, g: 0.25, b: 0.125, a: 1.0 });
    clear(&mut encoder, &source, wgpu::Color { r: 1.0, g: 0.5, b: 0.2, a: 1.0 });
    gpu.queue.submit([encoder.finish()]);
    let shape = Shape::rounded(8.0, 8.0, 48.0, 48.0, 8.0);
    let area = Rect::new(0.0, 0.0, SIZE as f32, SIZE as f32);
    println!("{:<28} {:>10} {:>10} {:>6} {:>8}", "effect", "first ms", "second ms", "drawn", "blurred");
    for (name, fx) in rows() {
        if only.as_deref().is_some_and(|o| !name.contains(o)) { continue; }
        let layer = fx.on(shape).source(0, area).at(0.5);
        let mut renderer = FxRenderer::new(&gpu.device, FORMAT, FxOptions::default());
        let first = frame(&gpu, &mut renderer, &layer, &backdrop, &source, &target);
        let second = first.as_ref().ok().map(|_| frame(&gpu, &mut renderer, &layer, &backdrop, &source, &target));
        match (first, second) {
            (Ok(a), Some(Ok(b))) => println!("{name:<28} {:>10.1} {:>10.1} {:>6} {:>8}", a.took.as_secs_f64() * 1e3, b.took.as_secs_f64() * 1e3, a.drawn, a.blurred),
            (Ok(a), Some(Err(e))) => { println!("{name:<28} {:>10.1} {:>10} device lost on the second frame: {e}", a.took.as_secs_f64() * 1e3, "LOST"); return; }
            (Err(e), _) => { println!("{name:<28} {:>10} device lost on the first frame: {e}", "LOST"); return; }
            (Ok(_), None) => unreachable!(),
        }
    }
}
