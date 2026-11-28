use rune::{El, Env, Runtime, col, el, row, text};
use rune_kit::{Kit, Spinner};

fn view(_: &(), _: &mut rune::Cx<()>) -> El {
    let k = Kit::neutral(true);
    let rows = (0..40).map(|i| {
        let lead = if i % 4 == 0 { Spinner::new().size(14.0).build(&k) } else { el().size(14.0, 14.0) };
        row().key(format!("row{i}")).w_full().h(36.0).gap(8.0).padding(8.0)
            .child(lead)
            .child(col().gap(2.0).child(text(format!("Session {i}"))).child(text(format!("Working on step {} of 12", i % 12))))
            .child(el().w_full())
            .child(el().size(48.0, 20.0).radius(6.0).fill(k.c.std_text.alpha(0.08)).child(text("Open")))
    });
    col().key("list").w_full().h_full().children(rows)
}

fn main() {
    assert!(!cfg!(debug_assertions), "build with --profile perf");
    let args: Vec<String> = std::env::args().collect();
    let frames: usize = args.iter().position(|a| a == "--frames").and_then(|i| args.get(i + 1)).map_or(600, |v| v.parse().unwrap());
    let mut rt = Runtime::new((), view, (480.0, 1500.0), Env::new(true, 2.0));
    rt.frame(0.0);
    for _ in 0..120 {
        rt.frame(1.0 / 60.0);
    }
    let mut samples: Vec<f64> = (0..frames).map(|_| {
        let start = rune::thread_cpu_ms();
        rt.frame(1.0 / 60.0);
        rune::thread_cpu_ms() - start
    }).collect();
    samples.sort_by(f64::total_cmp);
    let p50 = samples[frames / 2];
    let p95 = samples[((frames - 1) as f64 * 0.95).round() as usize];
    let mean = samples.iter().sum::<f64>() / frames as f64;
    println!("spinner-frames elements {} instances {} frame_cpu_ms p50 {p50:.4} p95 {p95:.4} mean {mean:.4}", rt.elements().len(), rt.instances.len());
}
