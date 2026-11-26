use super::*;
use super::gpu_tests::{Gpu, OFFSCREEN_FORMAT, bgra_ramp, draw, publish, render};
#[cfg(target_os = "macos")]
use super::gpu_tests::pixel;

macro_rules! on_each_backend {
    ($wgpu:ident, $metal:ident, $body:ident) => {
        #[test]
        fn $wgpu() {
            let Some(gpu) = Gpu::headless_wgpu() else { return };
            $body(gpu);
        }
        #[cfg(all(target_vendor = "apple", feature = "metal-backend"))]
        #[test]
        fn $metal() {
            let Some(gpu) = Gpu::try_new(rune_gpu::metal::instance()) else { return };
            $body(gpu);
        }
    };
}

const ORIENTS: [[i8; 4]; 8] =
    [[1, 0, 0, 1], [-1, 0, 0, 1], [1, 0, 0, -1], [-1, 0, 0, -1], [0, 1, 1, 0], [0, -1, 1, 0], [0, 1, -1, 0], [0, -1, -1, 0]];

/// The buffer pixel an upright pixel shows, by rotating about the centre.
fn buffer_pixel(o: Orient, [w, h]: [u32; 2], x: u32, y: u32) -> (u32, u32) {
    let [uw, uh] = o.upright([w, h]);
    let (cx, cy) = (x as f64 + 0.5 - uw as f64 / 2.0, y as f64 + 0.5 - uh as f64 / 2.0);
    let m = o.0.map(f64::from);
    ((m[0] * cx + m[1] * cy + w as f64 / 2.0).floor() as u32, (m[2] * cx + m[3] * cy + h as f64 / 2.0).floor() as u32)
}
/// The frame turned upright on the CPU.
fn turned(b: &ByteFrame, o: Orient) -> ByteFrame {
    let plane = |p: &[u8], w: u32, h: u32, bpp: usize| {
        let [uw, uh] = o.upright([w, h]);
        let mut out = Vec::with_capacity(p.len());
        for y in 0..uh {
            for x in 0..uw {
                let (bx, by) = buffer_pixel(o, [w, h], x, y);
                let i = (by * w + bx) as usize * bpp;
                out.extend_from_slice(&p[i..i + bpp]);
            }
        }
        out
    };
    let [w, h] = [b.width, b.height];
    let planes = match b.format {
        Format::Bgra => [plane(&b.planes[0], w, h, 4), vec![]],
        _ => [plane(&b.planes[0], w, h, 1), plane(&b.planes[1], w / 2, h / 2, 2)],
    };
    let [width, height] = o.upright([w, h]);
    ByteFrame { width, height, format: b.format, color: b.color, planes }
}
fn publish_oriented(s: &NativeSurface, b: ByteFrame, id: u64, native: bool, o: Orient) {
    #[cfg(target_os = "macos")]
    if native {
        s.push_oriented(macos::PixelBuffer::from_bytes(&b).unwrap(), id, o).unwrap();
        return;
    }
    let _ = native;
    s.push_bytes_oriented(b, id, o).unwrap();
}

#[test]
fn orientations_come_from_quarter_turn_transforms_only() {
    // An iPhone portrait clip: buffer (x, y) shows at (h - y, x).
    assert_eq!(Orient::from_transform(0.0, 1.0, -1.0, 0.0), Orient([0, 1, -1, 0]));
    assert_eq!(Orient::from_transform(1.0, 0.0, 0.0, 1.0), Orient::UP);
    assert_eq!(Orient::from_transform(-1.0, 0.0, 0.0, -1.0), Orient([-1, 0, 0, -1]));
    let s = std::f64::consts::FRAC_1_SQRT_2;
    assert_eq!(Orient::from_transform(s, s, -s, s), Orient::UP, "45 degrees isn't a quarter turn");
    assert_eq!(Orient::from_transform(2.0, 0.0, 0.0, 2.0), Orient::UP, "scales are ignored");
    assert!(Orient([0, 1, -1, 0]).swaps());
    assert_eq!(Orient([0, 1, -1, 0]).upright([64, 32]), [32, 64]);
    // The buffer's top-left pixel ends up top-right, as it does on the phone.
    assert_eq!(buffer_pixel(Orient([0, 1, -1, 0]), [6, 4], 3, 0), (0, 0));
}

fn bt709_steps(byte: u8) -> f32 {
    let x = rune_core::color::srgb_decode(byte as f32 / 255.0);
    (if x < 0.018 { 4.5 * x } else { 1.099 * x.powf(0.45) - 0.099 }) * 255.0
}

// A buffer drawn with an orientation equals the same picture turned upright on the CPU, for
// every quarter turn and flip, at 1:1, magnified and shrunk, BGRA and NV12.
on_each_backend!(oriented_frames_draw_like_frames_turned_upright, oriented_frames_on_metal, oriented_frames_draw_like_frames_turned_upright_on);
fn oriented_frames_draw_like_frames_turned_upright_on(gpu: Gpu) {
    let software = gpu.adapter.get_info().device_type == wgpu::DeviceType::Cpu;
    let mut renderer = SurfaceRenderer::new(&gpu.device, OFFSCREEN_FORMAT);
    let mut id = 0;
    for native in [false, cfg!(target_os = "macos")] {
        for m in ORIENTS {
            let o = Orient(m);
            for frame in [bgra_ramp(16, 8).0, synthetic(32, 16, 3)] {
                let filtered_in_8_bits = software && frame.format != Format::Bgra;
                let upright = turned(&frame, o);
                let [uw, uh] = [upright.width, upright.height];
                let (oriented, reference) = (NativeSurface::new(), NativeSurface::new());
                id += 1;
                publish_oriented(&oriented, frame, id, native, o);
                publish(&reference, upright, id, native);
                for (w, h, what) in [(uw, uh, "1:1"), (uw * 2, uh * 2, "2x"), (uw / 2, uh / 2, "half")] {
                    let rect = [0.0, 0.0, w as f32, h as f32];
                    let got = render(&gpu, &mut renderer, &[draw(&oriented, rect)], (w, h));
                    let want = render(&gpu, &mut renderer, &[draw(&reference, rect)], (w, h));
                    for (i, (g, r)) in got.iter().zip(&want).enumerate() {
                        let close = if filtered_in_8_bits { (bt709_steps(*g) - bt709_steps(*r)).abs() < 4.0 } else { g.abs_diff(*r) <= 1 };
                        assert!(close, "native={native} {m:?} {what} byte {i}: {g} != {r}");
                    }
                }
            }
        }
    }
}

// A portrait clip stored sideways (display matrix rotate=90): its player buffers, drawn with
// the track's orientation, match the upright poster AVFoundation decodes, quadrant by quadrant.
#[cfg(target_os = "macos")]
on_each_backend!(rotated_clip_buffers_draw_upright_like_its_poster, rotated_clip_on_metal, rotated_clip_buffers_draw_upright_like_its_poster_on);
#[cfg(target_os = "macos")]
fn rotated_clip_buffers_draw_upright_like_its_poster_on(gpu: Gpu) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../rune/tests/fixtures/quadrants-rot90.mp4");
    let (upright, raw, o) = crate::video::test_frames(&crate::MediaSource::File(path), 0.125).unwrap();
    assert_eq!((raw.width, raw.height), (64, 32), "stored landscape");
    assert_eq!((upright.width, upright.height), (32, 64), "shown portrait");
    assert!(o.swaps(), "{o:?}");
    let mut renderer = SurfaceRenderer::new(&gpu.device, OFFSCREEN_FORMAT);
    for native in [false, true] {
        let (oriented, poster) = (NativeSurface::new(), NativeSurface::new());
        publish_oriented(&oriented, ByteFrame { planes: raw.planes.clone(), ..raw }, 1, native, o);
        publish(&poster, ByteFrame { planes: upright.planes.clone(), ..upright }, 1, native);
        for scale in [1, 2] {
            let (w, h) = (32 / scale, 64 / scale);
            let rect = [0.0, 0.0, w as f32, h as f32];
            let got = render(&gpu, &mut renderer, &[draw(&oriented, rect)], (w, h));
            let want = render(&gpu, &mut renderer, &[draw(&poster, rect)], (w, h));
            // Inside each quadrant, clear of chroma bleed at its edges.
            let mut colours = Vec::new();
            for (qx, qy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let (x, y) = ((qx * 16 + 8) / scale, (qy * 32 + 16) / scale);
                let (g, r) = (pixel(&got, w, x, y), pixel(&want, w, x, y));
                assert!(g.iter().zip(r).all(|(g, r)| g.abs_diff(r) <= 8), "native={native} 1/{scale} quadrant ({qx},{qy}): {g:?} != {r:?}");
                colours.push(r);
            }
            colours.dedup();
            assert_eq!(colours.len(), 4, "four distinct quadrants: {colours:?}");
        }
    }
}
