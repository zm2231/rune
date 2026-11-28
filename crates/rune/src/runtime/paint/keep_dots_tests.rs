use super::*;
use crate::{Color, Cx, CycleDot, El, Env, Runtime, ScrollSpec, col, el, row, text};

const STOPS: &[(f32, f32)] = &[(0.0, 0.2), (0.4, 1.0), (1.0, 0.2)];

fn dots() -> &'static [CycleDot] {
    static DOTS: std::sync::OnceLock<Vec<CycleDot>> = std::sync::OnceLock::new();
    DOTS.get_or_init(|| {
        (0..9).map(|k| {
            let cycle = crate::Cycle { period: 1.8, phase: k as f32 / 9.0, stops: STOPS, still: 0.5, fps: 60.0 };
            CycleDot { rect: [(k % 3) as f32 * 0.375 + 0.0625, (k / 3) as f32 * 0.375 + 0.0625, 0.125, 0.125], radius: 0.24, cycle }
        }).collect()
    })
}

const ROWS: usize = 12;

fn list(_: &(), _: &mut Cx<()>) -> El {
    let rows = (0..ROWS).map(|i| {
        let lead = if i % 3 == 0 { el().key("spin").size(14.0, 14.0).pixel_snap(false).fill(Color::WHITE).cycle_dots(dots()) } else { el().size(14.0, 14.0) };
        row().key(format!("row{i}")).w_full().h(30.0).gap(6.0)
            .child(lead)
            .child(text(format!("Row {i}")))
            .child(el().size(40.0, 20.0).fill(Color::WHITE.alpha(0.1)).child(text("Open")))
    });
    col().key("list").w_full().h_full().scroll(ScrollSpec::default()).children(rows)
}

fn screen() -> Runtime<()> {
    let mut rt = Runtime::new((), list, (300.0, 200.0), Env::new(true, 2.0));
    rt.frame(0.0);
    for _ in 0..3 {
        rt.frame(1.0 / 60.0);
    }
    rt
}

fn spinner_rows(rt: &Runtime<()>) -> Vec<usize> {
    let nodes = &rt.core().tree.nodes;
    (0..nodes.len()).filter(|&i| nodes[i].path.ends_with("/spin")).filter_map(|i| nodes[i].parent).collect()
}

#[test]
fn rows_holding_a_dots_loop_are_copied_whole_and_draw_what_a_full_paint_draws() {
    let (mut copy, mut fresh) = (screen(), screen());
    copy.core_mut().paint.keep.force_copy = true;
    let visible = spinner_rows(&copy).into_iter().filter(|&r| copy.core().paint.seen[r]).count();
    assert!(visible >= 2, "{visible} spinner rows on screen");
    for f in 0..120 {
        if f == 60 {
            for rt in [&mut copy, &mut fresh] {
                rt.wheel(150.0, 100.0, 40.0);
            }
        }
        copy.frame(1.0 / 60.0);
        fresh.core_mut().paint.keep.off_once = true;
        fresh.frame(1.0 / 60.0);
        let (a, b) = (copy.core(), fresh.core());
        assert_eq!(bytemuck::cast_slice::<Instance, u8>(&a.instances), bytemuck::cast_slice::<Instance, u8>(&b.instances), "frame {f}: copied instances differ from a full paint");
        assert_eq!(a.cycle_paint().1, b.cycle_paint().1, "frame {f}: the dots' slots differ from a full paint's");
        assert_eq!(a.wake_deadline(), b.wake_deadline(), "frame {f}: the dots' wake differs from a full paint's");
        if f == 30 {
            for r in spinner_rows(&copy) {
                assert!(a.paint.keep.clean[r], "{} holds a dots loop and is copied whole", a.tree.nodes[r].path);
            }
        }
    }
}
