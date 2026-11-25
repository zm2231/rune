use super::icon::{
    AttachPlacement, compute_placement, integer_buffer_scale, prepare_icon,
    rgba_to_premultiplied_argb8888,
};
use crate::drag_out::DragImage;

#[test]
fn buffer_scale_only_applies_when_integer_scale_divides_both_dimensions() {
    assert_eq!(integer_buffer_scale(64, 64, 1.0), 1);
    assert_eq!(integer_buffer_scale(64, 64, 2.0), 2);
    assert_eq!(integer_buffer_scale(63, 64, 2.0), 1);
    assert_eq!(integer_buffer_scale(64, 63, 2.0), 1);
    assert_eq!(integer_buffer_scale(90, 60, 3.0), 3);
    assert_eq!(integer_buffer_scale(64, 64, 1.5), 1);
}

#[test]
fn placement_uses_offset_on_v5_and_attach_on_v4() {
    let frame = [20.0, 40.0, 64.0, 32.0];
    let press = (32.0, 50.0);
    let v4 = compute_placement(128, 64, 2.0, frame, press, 4);
    assert_eq!(v4.buffer_scale, 2);
    assert_eq!(v4.attach, AttachPlacement::AttachOffset { dx: -12, dy: -10 });

    let v5 = compute_placement(128, 64, 2.0, frame, press, 5);
    assert_eq!(v5.buffer_scale, 2);
    assert_eq!(v5.attach, AttachPlacement::SurfaceOffset { dx: -12, dy: -10 });
}

#[test]
fn rgba_to_premultiplied_argb8888_premultiplies_alpha() {
    let rgba = [255, 0, 128, 255, 255, 128, 64, 0, 200, 100, 50, 128];
    let argb = rgba_to_premultiplied_argb8888(&rgba);
    assert_eq!(argb.len(), 12);
    let px0 = u32::from_ne_bytes(argb[0..4].try_into().unwrap());
    let px1 = u32::from_ne_bytes(argb[4..8].try_into().unwrap());
    let px2 = u32::from_ne_bytes(argb[8..12].try_into().unwrap());
    assert_eq!(px0, 0xFFFF_0080);
    assert_eq!(px1, 0x0000_0000);
    assert_eq!(px2 >> 24, 128);
    assert_eq!((px2 >> 16) & 0xff, 100);
    assert_eq!((px2 >> 8) & 0xff, 50);
    assert_eq!(px2 & 0xff, 25);
}

#[test]
fn prepare_icon_decodes_png_and_computes_centered_and_element_offsets() {
    let rgba = vec![255u8; 40 * 20 * 4];
    let img = DragImage::rgba(40, 20, &rgba).unwrap();
    let prepared = prepare_icon(&img, [0.0, 0.0, 100.0, 50.0], (50.0, 25.0), 2.0).unwrap();
    assert_eq!((prepared.width, prepared.height, prepared.buffer_scale), (40, 20, 2));
    // Centered 20x10 pt icon under press (50, 25) -> top-left (40, 20) -> offset (-10, -5).
    assert_eq!(prepared.offset, (-10, -5));

    let over = img.over_element();
    let prepared_el = prepare_icon(&over, [10.0, 5.0, 100.0, 50.0], (30.0, 20.0), 2.0).unwrap();
    // Anchored at element top-left (10, 5), press at (30, 20) -> offset (-20, -15).
    assert_eq!(prepared_el.offset, (-20, -15));
}
