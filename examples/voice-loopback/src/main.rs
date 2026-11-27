mod session;

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::session::Session;
    use rune::{MicPermission, Voice};
    use std::process::exit;
    use std::sync::mpsc;
    use std::time::Duration;

    const TICK: Duration = Duration::from_millis(5);

    struct Args {
        loopback: bool,
        delay_ms: u64,
        seconds: Option<u64>,
    }

    fn args() -> Args {
        let mut args = Args { loopback: true, delay_ms: 300, seconds: None };
        let mut it = std::env::args().skip(1);
        while let Some(arg) = it.next() {
            match arg.as_str() {
                "--burst-only" => args.loopback = false,
                "--delay-ms" => args.delay_ms = it.next().and_then(|v| v.parse().ok()).unwrap_or(args.delay_ms),
                "--seconds" => args.seconds = it.next().and_then(|v| v.parse().ok()),
                _ => {
                    eprintln!("usage: voice-loopback [--burst-only] [--delay-ms N] [--seconds N]");
                    exit(2);
                }
            }
        }
        args
    }

    fn permission() {
        match Voice::permission() {
            MicPermission::Granted => {}
            MicPermission::Undetermined => {
                let (tx, rx) = mpsc::channel();
                Voice::request_permission(move |answer| {
                    tx.send(answer).ok();
                });
                if rx.recv() != Ok(MicPermission::Granted) {
                    eprintln!("microphone access was not granted");
                    exit(1);
                }
            }
            other => {
                let settings = if cfg!(target_os = "windows") {
                    "Settings > Privacy & security > Microphone"
                } else {
                    "System Settings > Privacy & Security > Microphone"
                };
                eprintln!("microphone access is {other:?}; allow it in {settings}");
                exit(1);
            }
        }
    }

    pub fn main() {
        let args = args();
        let mut session = match Session::open(args.loopback, args.delay_ms) {
            Ok(session) => session,
            Err(e) => {
                eprintln!("{e}");
                exit(1);
            }
        };
        permission();
        session.start();
        println!("{}", session.banner());
        let mut log = Vec::new();
        let mut next_report = Duration::from_secs(1);
        loop {
            session.tick(&mut log);
            for line in log.drain(..) {
                println!("{line}");
            }
            if session.elapsed() >= next_report {
                next_report += Duration::from_secs(1);
                println!("{}", session.report());
            }
            if args.seconds.is_some_and(|s| session.elapsed() >= Duration::from_secs(s)) {
                break;
            }
            std::thread::sleep(TICK);
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod web {
    use super::session::Session;
    use std::cell::RefCell;
    use wasm_bindgen::prelude::wasm_bindgen;

    thread_local! {
        static SESSION: RefCell<Option<Session>> = const { RefCell::new(None) };
    }

    #[wasm_bindgen]
    pub fn start(loopback: bool) -> String {
        SESSION.with_borrow_mut(|slot| {
            *slot = None;
            match Session::open(loopback, 300) {
                Ok(session) => {
                    session.start();
                    let banner = session.banner();
                    *slot = Some(session);
                    banner
                }
                Err(e) => e.to_string(),
            }
        })
    }

    #[wasm_bindgen]
    pub fn stop() {
        SESSION.with_borrow_mut(|slot| *slot = None);
    }

    #[wasm_bindgen]
    pub fn tick() -> String {
        SESSION.with_borrow_mut(|slot| {
            let mut log = Vec::new();
            if let Some(session) = slot {
                session.tick(&mut log);
            }
            log.join("\n")
        })
    }

    #[wasm_bindgen]
    pub fn report() -> String {
        SESSION.with_borrow_mut(|slot| slot.as_mut().map(Session::report).unwrap_or_default())
    }
}

fn main() {
    #[cfg(not(target_arch = "wasm32"))]
    native::main();
}
