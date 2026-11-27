#[cfg(any(target_os = "windows", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SystemAec {
    pub(crate) on: bool,
    pub(crate) can_set: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(target_os = "linux", allow(dead_code))]
pub(crate) enum EchoPath {
    System,
    TurnOnSystem,
    InProcess,
}

#[cfg(any(target_os = "windows", test))]
pub(crate) fn choose(system: Option<SystemAec>) -> EchoPath {
    match system {
        Some(SystemAec { on: true, .. }) => EchoPath::System,
        Some(SystemAec { on: false, can_set: true }) => EchoPath::TurnOnSystem,
        Some(SystemAec { on: false, can_set: false }) | None => EchoPath::InProcess,
    }
}
