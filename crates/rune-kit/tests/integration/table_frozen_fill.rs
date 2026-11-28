use std::borrow::Cow;

use rune::render::{Gpu, OFFSCREEN_FORMAT, Offscreen, Renderer};
use rune::runtime::ElementInfo;
use rune::{Cx, El, Env, Runtime, col};
use rune_kit::{Cell, Kit, Table, TableColumn, TableState};

const W: u32 = 360;
const H: u32 = 240;

struct App {
    table: TableState,
}

fn view(s: &App, _: &mut Cx<App>) -> El {
    let k = Kit::neutral(false);
    let mut t = Table::new(|s: &App| &s.table, |s: &mut App| &mut s.table, |_: &App| 4, |_: &App, i| Cow::Owned(format!("r{i}")))
        .scroll_wide(true)
        .frozen_columns(1)
        .zebra(true);
    for c in 0..6 {
        t = t.column(TableColumn::new(format!("c{c}"), format!("Col {c}")).width(100.0));
    }
    col().size(W as f32, H as f32).fill(k.c.bg_base).child(t.cell(|_: &App, _, _| Cell::text("")).build(s, &k).h(H as f32))
}

fn settle(rt: &mut Runtime<App>) {
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
}

fn cell(rt: &Runtime<App>, column: &str, row: usize) -> ElementInfo {
    let mut cells: Vec<ElementInfo> =
        rt.elements().into_iter().filter(|e| !e.exiting && e.role == "gridcell" && e.key.as_deref() == Some(column)).collect();
    cells.sort_by(|a, b| a.rect[1].total_cmp(&b.rect[1]));
    cells.remove(row)
}

#[test]
fn frozen_striped_and_selected_cells_draw_the_colour_of_the_cells_beside_them() {
    let Some(gpu) = Gpu::test_headless() else { return };
    let mut rt = Runtime::new(App { table: TableState::new().key("frz") }, view, (W as f32, H as f32), Env::new(false, 1.0));
    assert!(rt.srgb_blend(), "a runtime blends in sRGB as a window does by default");
    settle(&mut rt);
    let selected = cell(&rt, "c3", 2).rect;
    rt.pointer_down(selected[0] + selected[2] / 2.0, selected[1] + selected[3] / 2.0);
    rt.pointer_up(selected[0] + selected[2] / 2.0, selected[1] + selected[3] / 2.0);
    rt.pointer_move(-50.0, -50.0);
    settle(&mut rt);
    assert_eq!(rt.state.table.selected().collect::<Vec<_>>(), ["r2"]);

    let mut renderer = Renderer::new(&gpu.device, OFFSCREEN_FORMAT);
    let target = Offscreen::new(&gpu.device, W, H);
    renderer.render_runtime_to_offscreen(&gpu.device, &gpu.queue, &target, &mut rt);
    let pixels = target.read(&gpu.device);
    let at = |e: &ElementInfo| {
        let (x, y) = ((e.rect[0] + e.rect[2] / 2.0) as usize, (e.rect[1] + e.rect[3] / 2.0) as usize);
        let i = (y * W as usize + x) * 4;
        [pixels[i], pixels[i + 1], pixels[i + 2]]
    };
    let plain = at(&cell(&rt, "c3", 0));
    for (row, what) in [(1, "striped"), (2, "selected")] {
        let (frozen, scrolling) = (at(&cell(&rt, "c0", row)), at(&cell(&rt, "c3", row)));
        assert_ne!(scrolling, plain, "the {what} row is tinted");
        let off = frozen.iter().zip(scrolling).map(|(a, b)| a.abs_diff(b)).max().unwrap();
        assert!(off <= 1, "{what} row: frozen cell draws {frozen:?}, the cell beside it {scrolling:?}");
    }
}
