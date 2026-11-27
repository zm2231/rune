mod drive;
#[cfg(target_os = "ios")]
mod native;
mod probe;
mod rows;
mod stats;
#[cfg(target_os = "ios")]
mod uikit;

use std::borrow::Cow;
use std::cell::RefCell;
use std::sync::OnceLock;
use std::time::Duration;

use rune::time::{Instant, SystemTime};

use rune::{App, AppSpec, Color, Cx, El, GrowthSpec, FrameEncoded, FrameOutcome, FramePresented, ScrollSpec, Spring, TextStyle, Tx, col, el, row, text};
use stats::say;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Mode {
    Rune,
    Uikit,
    Native,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum DriveKind {
    Curve,
    Glide,
}

#[derive(Clone, Debug)]
struct Args {
    mode: Mode,
    auto_scroll: bool,
    drive: DriveKind,
    rows: usize,
    idle: f64,
    exit: bool,
}

const USAGE: &str = "ios-perf [--mode rune|uikit|native] [--auto-scroll] [--drive curve|glide] [--rows N] [--idle SECONDS] [--exit]";

impl Args {
    fn parse(argv: impl IntoIterator<Item = String>) -> Result<Args, String> {
        let mut args = Args { mode: Mode::Rune, auto_scroll: false, drive: DriveKind::Curve, rows: 3000, idle: 5.0, exit: false };
        let mut it = argv.into_iter().filter(|a| a != "--");
        while let Some(flag) = it.next() {
            let mut value = || it.next().ok_or_else(|| format!("{flag} needs a value"));
            match flag.as_str() {
                "--mode" => args.mode = match value()?.as_str() { "rune" => Mode::Rune, "uikit" => Mode::Uikit, "native" => Mode::Native, other => return Err(format!("unknown mode {other}")) },
                "--drive" => args.drive = match value()?.as_str() { "curve" => DriveKind::Curve, "glide" => DriveKind::Glide, other => return Err(format!("unknown drive {other}")) },
                "--rows" => args.rows = value()?.parse().map_err(|e| format!("--rows: {e}"))?,
                "--idle" => args.idle = value()?.parse().map_err(|e| format!("--idle: {e}"))?,
                "--auto-scroll" => args.auto_scroll = true,
                "--exit" => args.exit = true,
                _ => {}
            }
        }
        Ok(args)
    }
}

static ARGS: OnceLock<Args> = OnceLock::new();

fn args() -> &'static Args { ARGS.get().expect("main parses the arguments before the app starts") }

#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Drive {
    #[default]
    Idle,
    Requested,
    Running { start: f64, glides: usize },
    Settling { until: f64 },
    Done,
}

struct State {
    keys: Vec<String>,
    drive: Drive,
}

fn init() -> State { State { keys: (0..args().rows).map(|i| i.to_string()).collect(), drive: Drive::Idle } }

#[derive(Default)]
struct Rec {
    base: Option<Instant>,
    main_wall: Option<SystemTime>,
    first_present: bool,
    driving: bool,
    seqs: Option<(u64, u64)>,
    presented: Vec<f64>,
    skipped: usize,
    targets: Vec<f64>,
    update_ms: Vec<f64>,
    encode_ms: Vec<f64>,
    refresh_hz: Vec<f64>,
    link: Vec<f64>,
    link_hz: Vec<f64>,
}

impl Rec {
    fn secs(&self, at: Instant) -> f64 { self.base.map_or(0.0, |base| at.saturating_duration_since(base).as_secs_f64()) }
}

thread_local! {
    static REC: RefCell<Rec> = RefCell::new(Rec::default());
}

fn rgb([r, g, b]: [u8; 3]) -> Color { rune::color::hex(&format!("#{r:02x}{g:02x}{b:02x}")).unwrap_or(Color::BLACK) }

fn list_row(_: &State, i: usize) -> El {
    row().w_full().h(drive::ROW_HEIGHT).px(16.0).gap(12.0).center_y()
        .child(el().size(28.0, 28.0).radius(14.0).fill(rgb(rows::swatch(i))))
        .child(col().grow(1.0).shrink().gap(2.0)
            .child(text(rows::title(i)).font(TextStyle::new(rows::TITLE_SIZE, 400.0)).color(rgb(rows::TITLE_RGB)).truncate_fade())
            .child(text(rows::detail(i)).font(TextStyle::new(rows::DETAIL_SIZE, 400.0)).color(rgb(rows::DETAIL_RGB)).truncate_fade()))
}

fn view(s: &State, cx: &mut Cx<State>) -> El {
    let background = rgb(rows::BACKGROUND_RGB);
    if args().mode == Mode::Uikit { return el().w_full().h_full().fill(background); }
    let [top, _, bottom, _] = cx.safe_area();
    let list = col().key("rows").w_full().pt(top).pb(bottom).growth(GrowthSpec::default())
        .virtual_rows(s.keys.len(), drive::ROW_HEIGHT, |s: &State, i| Cow::Borrowed(s.keys[i].as_str()), list_row);
    col().key("list").w_full().h_full().fill(background).scroll(ScrollSpec { stick_bottom: false }).child(list)
}

fn set_driving(on: bool) { REC.with(|r| r.borrow_mut().driving = on); }

fn poll(s: &mut State, clock: f64, tx: &mut Tx) -> bool {
    if s.drive == Drive::Requested {
        set_driving(true);
        #[cfg(target_os = "ios")]
        watch_link();
        s.drive = Drive::Running { start: clock, glides: 0 };
    }
    match &mut s.drive {
        Drive::Running { start, glides } => {
            let t = clock - *start;
            if t >= drive::total_seconds() {
                set_driving(false);
                s.drive = Drive::Settling { until: clock + 0.5 };
                tx.wake_at(clock + 0.5);
                return false;
            }
            match args().drive {
                DriveKind::Curve => {
                    tx.scroll_to_y("list", drive::offset_at(t) as f32);
                    tx.wake_at(clock);
                }
                DriveKind::Glide => {
                    let targets = drive::glide_targets();
                    if *glides <= (t / drive::fling_seconds()) as usize && *glides < targets.len() {
                        tx.scroll_to_with("list", 0.0, targets[*glides] as f32, Spring::new(1.2, 0.0));
                        *glides += 1;
                    }
                    tx.wake_at(*start + *glides as f64 * drive::fling_seconds());
                }
            }
        }
        Drive::Settling { until } if clock < *until => tx.wake_at(*until),
        Drive::Settling { .. } => {
            s.drive = Drive::Done;
            report_rune();
            finish();
        }
        Drive::Idle | Drive::Requested | Drive::Done => {}
    }
    false
}

fn on_encoded(f: &FrameEncoded) {
    if f.outcome() != FrameOutcome::Drawn { return; }
    REC.with(|r| {
        let mut r = r.borrow_mut();
        if !r.driving { return; }
        r.seqs = Some(r.seqs.map_or((f.seq(), f.seq()), |(first, _)| (first, f.seq())));
        if let Some(target) = f.presents_at() { let t = r.secs(target); r.targets.push(t); }
        if let Some(ms) = f.update_ms() { r.update_ms.push(ms); }
        if let Some(ms) = f.encode_ms() { r.encode_ms.push(ms); }
        if let Some(hz) = f.refresh_hz() { r.refresh_hz.push(hz); }
    });
}

fn on_presented(p: &FramePresented, started: &mut Option<std::sync::mpsc::Sender<()>>) {
    let first = REC.with(|r| {
        let mut r = r.borrow_mut();
        if let Some((first, last)) = r.seqs && (first..=last).contains(&p.seq()) {
            match p.presented_at() {
                Some(at) => { let t = r.secs(at); r.presented.push(t); }
                None => r.skipped += 1,
            }
        }
        let first = !r.first_present && p.presented_at().is_some();
        if first { r.first_present = true; }
        first.then(|| (p.presented_at(), r.main_wall))
    });
    let Some((Some(at), main_wall)) = first else { return };
    say_first_frame(probe::wall_of(at), main_wall);
    let marks: String = rune::marks::marks().into_iter()
        .map(|(name, at)| format!(" {name}={}", ms(probe::since_start_ms(probe::wall_of(at))))).collect();
    say(format!("PERF marks{marks}"));
    if let Some(started) = started.take() { started.send(()).ok(); }
}

fn say_first_frame(at: SystemTime, main_wall: Option<SystemTime>) {
    let main_ms = main_wall.and_then(probe::since_start_ms);
    say(format!("PERF first_frame_ms={} main_ms={} mode={:?}", ms(probe::since_start_ms(at)), ms(main_ms), args().mode).to_lowercase());
}

fn ms(v: Option<f64>) -> String { v.map_or_else(|| "nan".into(), |v| format!("{v:.1}")) }

fn report_rune() {
    REC.with(|r| {
        let r = r.borrow();
        let drive = format!(" drive={:?}", args().drive).to_lowercase();
        say(stats::scroll_line("rune", "presented", &r.presented, &format!(" skipped={}{drive}", r.skipped)));
        say(stats::scroll_line("rune", "submit_target", &r.targets, &drive));
        if !r.link.is_empty() { say(stats::scroll_line("rune", "link", &r.link, &link_extra(&r.link_hz))); }
        let refresh = stats::percentile(&stats::sorted(&r.refresh_hz), 50.0);
        say(format!("PERF rune_cpu{}{} refresh_hz_p50={refresh:.1}", stats::spread("update_ms", &r.update_ms), stats::spread("encode_ms", &r.encode_ms)));
    });
}

fn link_extra(hz: &[f64]) -> String {
    let s = stats::sorted(hz);
    format!(" link_hz_p50={:.1} link_hz_min={:.1}", stats::percentile(&s, 50.0), s.first().copied().unwrap_or(f64::NAN))
}

fn finish() {
    say(format!("PERF after_scroll_footprint_mib={:.2}", probe::footprint_mib()));
    if args().exit { std::process::exit(0); }
}

#[cfg(target_os = "ios")]
fn watch_link() {
    uikit::start_link(Box::new(|shown, target, link| {
        let done = REC.with(|r| {
            let mut r = r.borrow_mut();
            if r.driving {
                r.link.push(shown);
                if target > shown { r.link_hz.push(1.0 / (target - shown)); }
            }
            !r.driving && !r.link.is_empty()
        });
        if done { uikit::stop_link(link); }
    }));
}

#[cfg(target_os = "ios")]
fn drive_uikit() {
    let base = uikit::table_offset().unwrap_or(0.0);
    let mut start = None;
    uikit::start_link(Box::new(move |shown, target, link| {
        let start = *start.get_or_insert(target);
        let t = target - start;
        if t >= drive::total_seconds() {
            uikit::set_table_offset(base + drive::offset_at(drive::total_seconds()));
            uikit::stop_link(link);
            REC.with(|r| {
                let r = r.borrow();
                say(stats::scroll_line("uikit", "link", &r.link, &link_extra(&r.link_hz)));
            });
            finish();
            return;
        }
        uikit::set_table_offset(base + drive::offset_at(t));
        REC.with(|r| {
            let mut r = r.borrow_mut();
            r.link.push(shown);
            if target > shown { r.link_hz.push(1.0 / (target - shown)); }
        });
    }));
}

fn start_drive(s: &mut State) {
    match args().mode {
        Mode::Rune => s.drive = Drive::Requested,
        #[cfg(target_os = "ios")]
        Mode::Uikit | Mode::Native => drive_uikit(),
        #[cfg(not(target_os = "ios"))]
        Mode::Uikit | Mode::Native => say("PERF error=uikit and native modes run on iOS only"),
    }
}

#[cfg(target_os = "ios")]
fn say_display() {
    let (max_fps, plist) = uikit::display();
    say(format!("PERF display max_fps={max_fps} disable_min_frame_duration={plist}"));
}

fn on_ready() {
    #[cfg(target_os = "ios")]
    {
        say_display();
        if args().mode == Mode::Uikit && !uikit::show_table(args().rows) { say("PERF error=no window to show the table in"); }
    }
}

#[cfg(target_os = "ios")]
fn run_native() -> ! {
    native::run(|root| {
        say_display();
        uikit::show_table_in(root, args().rows);
        uikit::start_link(Box::new(|_, target, link| {
            uikit::stop_link(link);
            say_first_frame(native::wall_of_media(target), REC.with(|r| r.borrow().main_wall));
            native::after(args().idle, || {
                say(format!("PERF idle_footprint_mib={:.2} idle_s={}", probe::footprint_mib(), args().idle));
                if args().auto_scroll { drive_uikit(); }
            });
        }));
    })
}

fn main() {
    let base = rune::time::now();
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let parsed = Args::parse(argv).unwrap_or_else(|e| {
        eprintln!("{e}\n{USAGE}");
        std::process::exit(2);
    });
    ARGS.set(parsed).expect("arguments are parsed once");
    REC.with(|r| {
        let mut r = r.borrow_mut();
        r.base = Some(base);
        r.main_wall = Some(probe::wall_of(base));
    });

    #[cfg(target_os = "ios")]
    if args().mode == Mode::Native { run_native(); }

    let app = App::new(AppSpec { poll: Some(poll), ..AppSpec::new("Perf", (390.0, 844.0), init, view) });
    let handle = app.handle();
    let (started, first) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        if first.recv().is_err() { return; }
        std::thread::sleep(Duration::from_secs_f64(args().idle));
        say(format!("PERF idle_footprint_mib={:.2} idle_s={}", probe::footprint_mib(), args().idle));
        if args().auto_scroll { handle.update(|s, _| start_drive(s)); }
    });
    let mut started = Some(started);
    app.on_ready(|_| on_ready())
        .on_frame_encoded(on_encoded)
        .on_frame_presented(move |p| on_presented(p, &mut started))
        .run();
}
