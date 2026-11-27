#![forbid(unsafe_code)]

use rune::{AppSpec, Cx, El, Tx, col, el, row, text};
use rune_kit::{Button, Canvas, ImageZoom, Kit, KitCx, KitTheme, Preview, Stage, StageZoom, ZoomPan};

struct State {
    theme: KitTheme,
    pan: ZoomPan,
    stage: StageZoom,
    preview: ImageZoom,
    open: bool,
}

const PANE: [f32; 2] = [340.0, 220.0];

fn grid(k: &Kit, cols: usize, rows: usize, cell: f32) -> El {
    (0..rows).fold(col(), |grid, r| {
        grid.child((0..cols).fold(row(), |line, c| {
            line.child(
                el().size(cell, cell)
                    .center()
                    .fill(if (r + c) % 2 == 0 { k.c.layer_02 } else { k.c.layer_03 })
                    .child(text(format!("{}{}", (b'A' + r as u8) as char, c + 1)).color(k.c.text_base)),
            )
        }))
    })
}

fn caption(k: &Kit, s: String) -> El {
    text(s).font(k.ty.small).color(k.c.text_muted)
}

fn view(s: &State, cx: &mut Cx<State>) -> El {
    let [top, right, bottom, left] = cx.safe_area();
    cx.with_kit(&s.theme, |cx| {
        let kit = cx.kit();
        let k = &*kit;
        let canvas = Canvas::new(|s: &mut State| &mut s.pan, [960.0, 640.0], grid(k, 12, 8, 80.0)).label("Grid").build(&s.pan, PANE);
        let stage = Stage::new(|s: &mut State| &mut s.stage, [800.0, 600.0], grid(k, 10, 8, 80.0)).build(&s.stage, k, PANE);
        let open = Button::new("Open the image preview").on_press(|s: &mut State| s.open = true).build(k);
        let mut root = col()
            .key("root")
            .w_full()
            .h_full()
            .pt(top + 16.0)
            .pr(right + 16.0)
            .pb(bottom + 16.0)
            .pl(left + 16.0)
            .gap(12.0)
            .fill(k.c.bg_base)
            .child(caption(k, {
                let [x, y] = s.pan.to_content([PANE[0] / 2.0, PANE[1] / 2.0]);
                format!("Canvas: {:.0}%, centred on ({x:.0}, {y:.0})", s.pan.scale() * 100.0)
            }))
            .child(el().key("canvas").size(PANE[0], PANE[1]).child(canvas))
            .child(caption(k, format!("Stage: {}", if s.stage.zoomed() { "zoomed in" } else { "fitted" })))
            .child(el().key("stage").size(PANE[0], PANE[1]).child(stage))
            .child(row().child(open));
        if s.open {
            let size = cx.layout_size("root").unwrap_or([390.0, 760.0]);
            let preview = Preview::new(|s: &mut State| &mut s.preview, [800.0, 600.0], grid(k, 10, 8, 80.0))
                .on_close(|s: &mut State, _| s.open = false)
                .build(&s.preview, k, size, false);
            root = root.child(el().pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0)).child(preview));
        }
        root
    })
}

fn main() {
    let mut spec = AppSpec::new(
        "Zoom",
        (390.0, 760.0),
        || State { theme: KitTheme::neutral(), pan: ZoomPan::new(), stage: StageZoom::default(), preview: ImageZoom::new(), open: false },
        view,
    );
    spec.poll = Some(|s: &mut State, now: f64, tx: &mut Tx| s.preview.poll(now, tx));
    rune::App::new(spec).run();
}
