//! Adaptive view and simulation distance for a Horizon server, in the manner of the Paper plugin
//! ViewDistanceTweaks (clean-room: its public documentation and config file only).
//!
//! `engine` is the decision logic, `config` the settings. This file holds the one engine a server
//! runs and what only the running system can say (free memory).
pub mod command;
pub mod config;
pub mod diag;
pub mod engine;
pub mod relief;

pub use command::Reply;
pub use config::{Config, DEFAULT_TOML};
pub use engine::{Change, Distances, Engine, Event, Observation, PlayerSample};
use std::sync::Mutex;

static ENGINE: Mutex<Option<Engine>> = Mutex::new(None);
static RELIEF: Mutex<relief::Relief> = Mutex::new(relief::Relief { changed: None });

/// Diagnostics at run time: `/vdt diagnose on|off` until the next restart (None follows the
/// config), and the unix seconds of the last regular and extra sample.
struct DiagState {
    switched: Option<bool>,
    last_sample: u64,
    last_extra: u64,
    pruned_day: u64,
}
static DIAG: Mutex<DiagState> = Mutex::new(DiagState { switched: None, last_sample: 0, last_extra: 0, pruned_day: 0 });

fn diag_state() -> std::sync::MutexGuard<'static, DiagState> {
    DIAG.lock().unwrap_or_else(|e| e.into_inner())
}

/// `/vdt diagnose ...`: switch diagnostics or write a note into `dir` at `unix` seconds.
pub fn diagnose(d: &command::Diagnose, dir: &std::path::Path, unix: u64) {
    match d {
        command::Diagnose::On => diag_state().switched = Some(true),
        command::Diagnose::Off => diag_state().switched = Some(false),
        command::Diagnose::Note(text) => {
            let max = engine().as_ref().map_or(50.0, |e| e.config().diag_max_file_mb);
            let _ = diag::append(dir, unix, &diag::note_line(unix, text), false, max);
        }
    }
}

/// Once a tick: writes `change` and, every `interval-seconds` (or above
/// `extra-sample-above-mspt`, at most every 10 s), a sample into `dir`. Nothing when off.
pub fn diagnostics(dir: &std::path::Path, unix: u64, change: Option<Change>, mspt: f64, sample: impl FnOnce() -> diag::Sample) {
    let cfg = match engine().as_ref() {
        Some(e) => e.config().clone(),
        None => return,
    };
    let mut st = diag_state();
    if !st.switched.unwrap_or(cfg.diagnostics) {
        return;
    }
    let max = cfg.diag_max_file_mb;
    if let Some(c) = change.filter(|_| cfg.diag_record_changes) {
        let _ = diag::append(dir, unix, &diag::change_line(unix, &c), false, max);
    }
    let regular = unix.saturating_sub(st.last_sample) >= cfg.diag_interval_seconds;
    let extra = cfg.diag_extra_above_mspt > 0.0 && mspt > cfg.diag_extra_above_mspt && unix.saturating_sub(st.last_extra) >= 10;
    if cfg.diag_record_samples && (regular || extra) {
        if regular {
            st.last_sample = unix;
        } else {
            st.last_extra = unix;
        }
        let _ = diag::append(dir, unix, &diag::sample_line(unix, &sample()), true, max);
    }
    if unix / 86_400 != st.pruned_day {
        st.pruned_day = unix / 86_400;
        diag::prune(dir, unix, cfg.diag_keep_days);
    }
}

fn engine() -> std::sync::MutexGuard<'static, Option<Engine>> {
    ENGINE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Loads (or reloads) the config text. Returns the problems found in it.
pub fn load_config(text: &str) -> Vec<String> {
    let (cfg, bad) = Config::parse(text);
    let mut engine = engine();
    match engine.as_mut() {
        Some(e) => e.set_config(cfg),
        None => *engine = Some(Engine::new(cfg)),
    }
    bad
}

/// One server tick; see [`Engine::tick`]. Runs on the defaults when no config was loaded.
pub fn tick(actual: Distances, observe: impl FnOnce() -> Observation) -> Option<Change> {
    engine().get_or_insert_with(|| Engine::new(Config::default())).tick(actual, observe)
}

/// `/vdt ...` (without the slash); None when the line is not ours. See [`command::run`].
pub fn command(line: &str, actual: Distances, observation: &Observation) -> Option<Reply> {
    let mut guard = engine();
    let e = guard.get_or_insert_with(|| Engine::new(Config::default()));
    let actual = e.current().unwrap_or(actual);
    command::run(e, line, actual, observation)
}

/// The personal throttles; see [`Engine::personal_tick`].
pub fn personal(observe: impl FnOnce() -> Vec<PlayerSample>) -> Vec<(u128, Option<Distances>)> {
    engine().as_mut().map_or_else(Vec::new, |e| e.personal_tick(observe))
}

/// Whether the next [`personal`] call looks at the players (so the caller builds its samples, and
/// asks for permissions, only then).
pub fn personal_due() -> bool {
    engine().as_ref().is_some_and(Engine::personal_due)
}

/// The permission prefixes of `per-player-fixed-view-distance` and `-simulation-distance`, for the
/// features that are on (each ends in a dot).
pub fn fixed_prefixes() -> (Option<String>, Option<String>) {
    engine().as_ref().map_or((None, None), |e| {
        let c = e.config();
        (c.fixed_view.then(|| c.fixed_view_prefix.clone()), c.fixed_sim.then(|| c.fixed_sim_prefix.clone()))
    })
}

/// The fixed distance a player's permissions give: the highest `<prefix><n>`, n 2 to 32, that `has`
/// says they hold.
pub fn fixed_distance(prefix: &str, has: impl Fn(&str) -> bool) -> Option<i32> {
    (2..=32).rev().find(|n| has(&format!("{prefix}{n}")))
}

/// The average and the sorted values, in milliseconds, of the server's last 100 tick times in
/// nanoseconds as the game lays them out (0 where no tick was recorded yet).
pub fn tick_stats(nanos: &[u64]) -> (f64, Vec<f64>) {
    let mut ms: Vec<f64> = nanos.iter().filter(|n| **n > 0).map(|n| *n as f64 / 1e6).collect();
    ms.sort_by(f64::total_cmp);
    let avg = if ms.is_empty() { 0.0 } else { ms.iter().sum::<f64>() / ms.len() as f64 };
    (avg, ms)
}

/// The players whose personal distances are lowered now.
pub fn throttled() -> Vec<(u128, Distances, &'static str)> {
    engine().as_ref().map_or_else(Vec::new, Engine::throttled)
}

/// What the game should do besides changing distances, since the last call.
pub fn take_events() -> Vec<Event> {
    engine().as_mut().map_or_else(Vec::new, Engine::take_events)
}

/// The operator levels for change notices and for `/vdt`.
pub fn permissions() -> (u8, u8) {
    engine().as_ref().map_or((1, 1), |e| (e.config().notify_min_permission, e.config().command_min_permission))
}

/// The emergency relief switches: (clear the weather, stop the day cycle).
pub fn relief_switches() -> (bool, bool) {
    engine().as_ref().map_or((true, true), |e| (e.config().relief_clear_weather, e.config().relief_freeze_time))
}

/// Emergency relief starts with the world in this state: the console commands to run.
pub fn relief_start(raining: bool, thundering: bool, advance_time: bool) -> Vec<String> {
    let (weather, time) = relief_switches();
    RELIEF.lock().unwrap_or_else(|e| e.into_inner()).start(weather, time, raining, thundering, advance_time)
}

/// Emergency relief ends, or the server stops: the console commands that put the world back.
pub fn relief_end() -> Vec<String> {
    RELIEF.lock().unwrap_or_else(|e| e.into_inner()).end()
}

/// Whether emergency relief has changed the world now.
pub fn relief_active() -> bool {
    RELIEF.lock().unwrap_or_else(|e| e.into_inner()).active()
}

/// The active profile ("normal" when none).
pub fn profile_name() -> String {
    engine().as_ref().map_or_else(|| "normal".to_owned(), |e| e.profile().to_owned())
}

/// `&` colour codes as the section signs the game's legacy text reads; a lone `&` stays.
pub fn to_section(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        let code = chars.peek().is_some_and(|k| k.is_ascii_hexdigit() || matches!(k.to_ascii_lowercase(), 'k'..='o' | 'r'));
        out.push(if c == '&' && code { '\u{a7}' } else { c });
    }
    out
}

/// Whether the log should say when distances change.
pub fn log_changes() -> bool {
    engine().as_ref().is_none_or(|e| e.config().log_changes)
}

/// Free share of the system's memory (`MemAvailable` of `/proc/meminfo`); `None` off Linux.
pub fn free_memory_percent() -> Option<f64> {
    parse_meminfo(&std::fs::read_to_string("/proc/meminfo").ok()?)
}

fn parse_meminfo(text: &str) -> Option<f64> {
    let kb = |key: &str| -> Option<f64> {
        text.lines().find_map(|l| l.strip_prefix(key)?.trim().split_whitespace().next()?.parse().ok())
    };
    let total = kb("MemTotal:")?;
    (total > 0.0).then(|| kb("MemAvailable:").map(|a| a / total * 100.0))?
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The first `load_config` once locked the engine twice and hung the server at start.
    #[test]
    fn loading_a_config_into_an_empty_engine_returns() {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let first = load_config("ticks-per-check = 100\n");
            let second = load_config("ticks-per-check = 50\n");
            tx.send((first, second, engine().as_ref().map(|e| e.config().ticks_per_check))).ok();
        });
        let (first, second, ticks) = rx.recv_timeout(std::time::Duration::from_secs(5)).expect("load_config hung");
        assert!(first.is_empty() && second.is_empty());
        assert_eq!(ticks, Some(50));
    }

    #[test]
    fn the_highest_fixed_permission_wins() {
        let held = |node: &str| matches!(node, "p.fixed.10" | "p.fixed.20" | "p.fixed.99");
        assert_eq!(fixed_distance("p.fixed.", held), Some(20));
        assert_eq!(fixed_distance("p.fixed.", |_| false), None);
        assert_eq!(fixed_distance("p.fixed.", |n| n == "p.fixed.32"), Some(32));
    }

    #[test]
    fn tick_times_skip_unrecorded_slots_and_sort() {
        let (avg, ms) = tick_stats(&[4_000_000, 0, 2_000_000, 6_000_000]);
        assert_eq!(ms, vec![2.0, 4.0, 6.0]);
        assert_eq!(avg, 4.0);
        assert_eq!(tick_stats(&[0, 0]), (0.0, vec![]));
    }

    #[test]
    fn colour_codes_become_section_signs() {
        assert_eq!(to_section("&7[VDT] &fA & B &x"), "\u{a7}7[VDT] \u{a7}fA & B &x");
    }

    #[test]
    fn meminfo_gives_the_available_share() {
        let text = "MemTotal:       16000000 kB\nMemFree:  100 kB\nMemAvailable:    4000000 kB\n";
        assert_eq!(parse_meminfo(text), Some(25.0));
        assert_eq!(parse_meminfo("MemTotal: 1 kB\n"), None);
    }
}
