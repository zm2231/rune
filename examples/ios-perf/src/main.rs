mod drive;
#[cfg(target_os = "ios")]
mod native;
mod probe;
mod rows;
mod stats;
mod trace;
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
    first_encoded: bool,
    polls: usize,
    beat: f64,
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
        trace::step("drive_running", || format!("clock={clock:.3} drive={:?}", args().drive));
        set_driving(true);
        #[cfg(target_os = "ios")]
        watch_link();
        s.drive = Drive::Running { start: clock, glides: 0 };
    }
    match &mut s.drive {
        Drive::Running { start, glides } => {
            let t = clock - *start;
            beat(t);
            if t >= drive::total_seconds() {
                trace::step("drive_end", || format!("t={t:.3}"));
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
        Drive::Settling { until } if clock < *until => {
            trace::step("settling", || format!("clock={clock:.3} until={until:.3}"));
            tx.wake_at(*until);
        }
        Drive::Settling { .. } => {
            trace::step("done", || format!("clock={clock:.3}"));
            s.drive = Drive::Done;
            report_rune();
            finish();
        }
        Drive::Idle | Drive::Requested | Drive::Done => {}
    }
    false
}

fn beat(t: f64) {
    if !trace::on() { return; }
    REC.with(|r| {
        let mut r = r.borrow_mut();
        r.polls += 1;
        if t - r.beat < 1.0 { return; }
        r.beat = t;
        let (polls, encoded, presented, skipped, link) = (r.polls, r.targets.len(), r.presented.len(), r.skipped, r.link.len());
        trace::step("drive_beat", || format!("t={t:.3} polls={polls} encoded={encoded} presented={presented} skipped={skipped} link_ticks={link}"));
    });
}

fn on_encoded(f: &FrameEncoded, started: &mut Option<std::sync::mpsc::Sender<()>>) {
    if f.outcome() != FrameOutcome::Drawn { return; }
    if let Some(started) = started.take() { started.send(()).ok(); }
    REC.with(|r| {
        let mut r = r.borrow_mut();
        if !r.first_encoded {
            r.first_encoded = true;
            trace::step("first_encoded", || format!("seq={}", f.seq()));
            if let Some(target) = f.presents_at() { say_first_frame(probe::wall_of(target), "link_target", r.main_wall); }
        }
        if !r.driving { return; }
        r.seqs = Some(r.seqs.map_or((f.seq(), f.seq()), |(first, _)| (first, f.seq())));
        if let Some(target) = f.presents_at() { let t = r.secs(target); r.targets.push(t); }
        if let Some(ms) = f.update_ms() { r.update_ms.push(ms); }
        if let Some(ms) = f.encode_ms() { r.encode_ms.push(ms); }
        if let Some(hz) = f.refresh_hz() { r.refresh_hz.push(hz); }
    });
}

fn on_presented(p: &FramePresented) {
    let first = REC.with(|r| {
        let mut r = r.borrow_mut();
        if let Some((first, last)) = r.seqs && (first..=last).contains(&p.seq()) {
            match p.presented_at() {
                Some(at) => { let t = r.secs(at); r.presented.push(t); }
                None => r.skipped += 1,
            }
        }
        if !r.first_present && p.presented_at().is_none() { trace::step("present_without_time", || format!("seq={}", p.seq())); }
        let first = !r.first_present && p.presented_at().is_some();
        if first { r.first_present = true; }
        first.then(|| (p.presented_at(), r.main_wall))
    });
    let Some((Some(at), main_wall)) = first else { return };
    say_first_frame(probe::wall_of(at), "presented", main_wall);
    let marks: String = rune::marks::marks().into_iter()
        .map(|(name, at)| format!(" {name}={}", ms(probe::since_start_ms(probe::wall_of(at))))).collect();
    say(format!("PERF marks{marks}"));
}

fn say_first_frame(at: SystemTime, source: &str, main_wall: Option<SystemTime>) {
    let main_ms = main_wall.and_then(probe::since_start_ms);
    say(format!("PERF first_frame_ms={} main_ms={} mode={:?} source={source}", ms(probe::since_start_ms(at)), ms(main_ms), args().mode).to_lowercase());
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
    trace::step("finish", || format!("exit={}", args().exit));
    if args().exit { std::process::exit(0); }
}

#[cfg(target_os = "ios")]
fn watch_link() {
    trace::step("watch_link", String::new);
    uikit::start_link(Box::new(|shown, target, link| {
        let done = REC.with(|r| {
            let mut r = r.borrow_mut();
            if r.driving {
                r.link.push(shown);
                if target > shown { r.link_hz.push(1.0 / (target - shown)); }
            }
            !r.driving && !r.link.is_empty()
        });
        if done {
            trace::step("watch_link_stop", String::new);
            uikit::stop_link(link);
        }
    }));
}

#[cfg(target_os = "ios")]
fn drive_uikit() {
    let base = uikit::table_offset().unwrap_or(0.0);
    let mut start = None;
    trace::step("drive_uikit", || format!("base={base:.1}"));
    uikit::start_link(Box::new(move |shown, target, link| {
        let start = *start.get_or_insert_with(|| {
            trace::step("drive_running", || format!("target={target:.3}"));
            target
        });
        let t = target - start;
        if t >= drive::total_seconds() {
            trace::step("drive_end", || format!("t={t:.3}"));
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
        beat(t);
        REC.with(|r| {
            let mut r = r.borrow_mut();
            r.link.push(shown);
            if target > shown { r.link_hz.push(1.0 / (target - shown)); }
        });
    }));
}

fn start_drive(s: &mut State) {
    trace::step("drive_start", || format!("mode={:?}", args().mode));
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
    stats::set_max_fps(u32::try_from(max_fps).unwrap_or(120));
    say(format!("PERF display max_fps={max_fps} disable_min_frame_duration={plist} reduce_motion={}", uikit::reduce_motion()));
}

fn on_ready() {
    trace::step("ready", String::new);
    #[cfg(target_os = "ios")]
    {
        say_display();
        if args().mode == Mode::Uikit && !uikit::show_table(args().rows) { say("PERF error=no window to show the table in"); }
    }
}

#[cfg(target_os = "ios")]
fn run_native() -> ! {
    trace::step("native_main", String::new);
    native::run(|root| {
        trace::step("scene_connected", String::new);
        say_display();
        uikit::show_table_in(root, args().rows);
        trace::step("table_shown", String::new);
        uikit::start_link(Box::new(|_, target, link| {
            trace::step("first_link_tick", || format!("target={target:.3}"));
            uikit::stop_link(link);
            say_first_frame(native::wall_of_media(target), "link_target", REC.with(|r| r.borrow().main_wall));
            native::after(args().idle, || {
                trace::step("idle_sample", String::new);
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
    trace::step("main", || format!("{:?}", args()));
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
        if first.recv().is_err() {
            trace::step("idle_gave_up", String::new);
            return;
        }
        trace::step("idle_wait", String::new);
        std::thread::sleep(Duration::from_secs_f64(args().idle));
        trace::step("idle_sample", String::new);
        say(format!("PERF idle_footprint_mib={:.2} idle_s={}", probe::footprint_mib(), args().idle));
        if args().auto_scroll {
            trace::step("drive_asked", String::new);
            handle.update(|s, _| start_drive(s));
        }
    });
    trace::step("app_run", String::new);
    let mut started = Some(started);
    app.on_ready(|_| on_ready())
        .on_visibility(|_, visible, _| trace::step("visibility", || format!("visible={visible}")))
        .on_focus(|_, focused, _| trace::step("focus", || format!("focused={focused}")))
        .on_frame_encoded(move |f| on_encoded(f, &mut started))
        .on_frame_presented(on_presented)
        .run();
}
