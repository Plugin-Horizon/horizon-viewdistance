//! The mod's config: a small TOML subset (`[section]`, `key = value`, `#` comments), keys named
//! like the Paper plugin's. A bad value falls back to its default and is reported, never fatal.
use std::collections::HashMap;

/// The file written when `config/viewdistance.toml` is missing.
pub const DEFAULT_TOML: &str = include_str!("../default.toml");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Off,
    Proactive,
    Reactive,
    Mixed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmptyTarget {
    Min,
    Max,
    Keep,
}

/// A named overlay for a few tuning values, switched with `/vdt profile <name>`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Profile {
    pub name: String,
    pub decrease_mspt: Option<f64>,
    pub increase_mspt: Option<f64>,
    pub max_increase_step: Option<i32>,
    pub max_decrease_step: Option<i32>,
    pub passed_checks_for_increase: Option<u32>,
    pub passed_checks_for_decrease: Option<u32>,
    pub ticking_target: Option<f64>,
    pub non_ticking_target: Option<f64>,
    pub cap_view: Option<i32>,
    pub cap_sim: Option<i32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub enabled: bool,
    pub mode: Mode,
    pub start_delay: u64,
    pub ticks_per_check: u64,
    pub passed_checks_for_increase: u32,
    pub passed_checks_for_decrease: u32,
    pub log_changes: bool,
    // chat announcements and commands
    pub announce: bool,
    pub announce_format: String,
    pub notify_min_permission: u8,
    pub command_min_permission: u8,
    // proactive
    pub ticking_target: f64,
    pub non_ticking_target: f64,
    pub empty_world: EmptyTarget,
    // reactive
    pub increase_mspt: f64,
    pub decrease_mspt: f64,
    pub prioritize_simulation: bool,
    pub max_increase_step: i32,
    pub max_decrease_step: i32,
    pub revert_increase_if_overloaded: bool,
    pub prediction: bool,
    pub prediction_history_minutes: u64,
    pub prediction_min_samples: usize,
    pub variance: bool,
    pub variance_threshold: f64,
    pub variance_min_samples: usize,
    pub variance_block_increase: bool,
    pub variance_min_avg: f64,
    // safeguards
    pub gen_safeguard: bool,
    pub gen_chunks_per_second: f64,
    pub gen_lock_increase: bool,
    pub gen_emergency_view: i32,
    pub memory_safeguard: bool,
    pub memory_min_free_percent: f64,
    pub grace: bool,
    pub grace_sim: i32,
    pub grace_view: i32,
    pub grace_ticks: u64,
    // per-player throttles
    pub afk: bool,
    pub afk_minutes: f64,
    pub afk_sim: i32,
    pub afk_view: i32,
    pub ping: bool,
    pub ping_threshold_ms: f64,
    pub ping_checks: u32,
    pub ping_sim: i32,
    pub ping_view: i32,
    /// `per-player-fixed-view-distance` / `-simulation-distance`: a player holding the permission
    /// `<prefix><n>` (n 2 to 32, the highest wins) gets that distance.
    pub fixed_view: bool,
    pub fixed_view_prefix: String,
    pub fixed_sim: bool,
    pub fixed_sim_prefix: String,
    // density
    pub density: bool,
    pub cluster_radius: i32,
    pub cluster_threshold: u32,
    pub max_bonus: i32,
    // emergency relief
    pub relief: bool,
    pub relief_clear_weather: bool,
    pub relief_freeze_time: bool,
    pub relief_recover_mspt: f64,
    // diagnostics
    pub diagnostics: bool,
    pub diag_interval_seconds: u64,
    pub diag_extra_above_mspt: f64,
    pub diag_record_samples: bool,
    pub diag_record_changes: bool,
    pub diag_max_file_mb: f64,
    pub diag_keep_days: u64,
    // profiles; the caps are set only by an active profile
    pub profiles: Vec<Profile>,
    pub cap_view: Option<i32>,
    pub cap_sim: Option<i32>,
    // bounds
    pub min_sim: i32,
    pub max_sim: i32,
    pub min_view: i32,
    pub max_view: i32,
    /// `world-settings.default.chunk-weight`: scales the chunk counts of the world in the budget
    /// (0 leaves them out of it).
    pub chunk_weight: f64,
}

impl Default for Config {
    fn default() -> Self {
        Self::parse("").0
    }
}

impl Config {
    /// The config and the problems found in `text` (unknown value types, out-of-range numbers).
    pub fn parse(text: &str) -> (Self, Vec<String>) {
        let v = Values::read(text);
        let mut bad = Vec::new();
        let mut num = |key: &str, default: f64, min: f64| -> f64 {
            match v.get(key) {
                None => default,
                Some(raw) => match raw.parse::<f64>() {
                    Ok(n) if n >= min => n,
                    _ => {
                        bad.push(format!("{key}: expected a number >= {min}, got {raw}"));
                        default
                    }
                },
            }
        };
        let start_delay = num("start-delay", 2400.0, 0.0) as u64;
        let ticks_per_check = num("ticks-per-check", 200.0, 1.0) as u64;
        let passed_up = num("passed-checks-for-increase", 9.0, 1.0) as u32;
        let passed_down = num("passed-checks-for-decrease", 1.0, 1.0) as u32;
        let ticking_target = num("proactive-mode-settings.global-ticking-chunk-count-target", 5780.0, 0.0);
        let non_ticking_target =
            num("proactive-mode-settings.global-non-ticking-chunk-count-target", 6720.0, 0.0);
        let r = "reactive-mode-settings.";
        let increase_mspt = num(&format!("{r}increase-mspt-threshold"), 40.0, 0.0);
        let decrease_mspt = num(&format!("{r}decrease-mspt-threshold"), 47.0, 0.0);
        let max_increase_step = num(&format!("{r}max-increase-step"), 2.0, 1.0) as i32;
        let max_decrease_step = num(&format!("{r}max-decrease-step"), 4.0, 1.0) as i32;
        let t = format!("{r}mspt-tracker-settings.");
        let pr = format!("{t}mspt-prediction.");
        let prediction_history_minutes = num(&format!("{pr}history-length"), 30.0, 1.0) as u64;
        let prediction_min_samples = num(&format!("{pr}min-samples"), 5.0, 2.0) as usize;
        let va = format!("{t}mspt-variance.");
        let variance_threshold = num(&format!("{va}variance-threshold"), 8.0, 0.0);
        let variance_min_samples = num(&format!("{va}min-samples"), 20.0, 2.0) as usize;
        let variance_min_avg = num(&format!("{va}min-avg-mspt-to-trigger"), 25.0, 0.0);
        let gen_chunks_per_second = num("chunk-generation-safeguard.chunks-per-second-threshold", 8.0, 0.0);
        let gen_emergency_view = num("chunk-generation-safeguard.emergency-view-distance", 8.0, -1.0) as i32;
        let memory_min_free_percent = num("memory-safeguard.min-free-percent", 15.0, 0.0);
        let grace_sim = num("startup-grace.simulation-distance", 4.0, 2.0) as i32;
        let grace_view = num("startup-grace.view-distance", 6.0, 2.0) as i32;
        let grace_ticks = num("startup-grace.duration-ticks", 6000.0, 0.0) as u64;
        let afk_minutes = num("afk-player-throttling.afk-time-minutes", 5.0, 0.0);
        let afk_sim = num("afk-player-throttling.afk-simulation-distance", 4.0, 2.0) as i32;
        let afk_view = num("afk-player-throttling.afk-view-distance", 6.0, 2.0) as i32;
        let ping_threshold_ms = num("ping-throttling.ping-threshold-ms", 200.0, 0.0);
        let ping_checks = num("ping-throttling.checks-before-throttle", 3.0, 1.0) as u32;
        let ping_sim = num("ping-throttling.ping-simulation-distance", 4.0, 2.0) as i32;
        let ping_view = num("ping-throttling.ping-view-distance", 6.0, 2.0) as i32;
        let prefix = |key: &str, default: &str| -> String {
            let mut p = v.get(key).filter(|p| !p.is_empty()).unwrap_or(default).to_ascii_lowercase();
            if !p.ends_with('.') {
                p.push('.');
            }
            p
        };
        let fixed_view_prefix = prefix("per-player-fixed-view-distance.permission-prefix", "viewdistancetweaks.fixed.");
        let fixed_sim_prefix = prefix("per-player-fixed-simulation-distance.permission-prefix", "viewdistancetweaks.simfixed.");
        let cluster_radius = num("density-based-scaling.cluster-radius", 8.0, 0.0) as i32;
        let cluster_threshold = num("density-based-scaling.cluster-player-threshold", 5.0, 1.0) as u32;
        let max_bonus = num("density-based-scaling.max-bonus", 4.0, 0.0) as i32;
        let relief_recover_mspt = num("emergency-relief.recover-below-mspt", 40.0, 0.0);
        let diag_interval_seconds = num("diagnostics.interval-seconds", 60.0, 10.0) as u64;
        let diag_extra_above_mspt = num("diagnostics.extra-sample-above-mspt", 0.0, 0.0);
        let diag_max_file_mb = num("diagnostics.max-file-size-mb", 50.0, 0.0);
        let diag_keep_days = num("diagnostics.keep-days", 7.0, 1.0) as u64;
        let notify_min_permission = num("notify-min-permission", 1.0, 0.0).min(4.0) as u8;
        let command_min_permission = num("command-min-permission", 1.0, 0.0).min(4.0) as u8;
        let w = "world-settings.default.";
        let chunk_weight = num(&format!("{w}chunk-weight"), 1.0, 0.0);
        let mut min_sim = num(&format!("{w}simulation-distance.min-simulation-distance"), 4.0, 2.0) as i32;
        let mut max_sim = num(&format!("{w}simulation-distance.max-simulation-distance"), 12.0, 2.0) as i32;
        let mut min_view = num(&format!("{w}view-distance.min-view-distance"), 6.0, 2.0) as i32;
        let mut max_view = num(&format!("{w}view-distance.max-view-distance"), 16.0, 2.0) as i32;
        if min_sim > max_sim || min_view > max_view {
            bad.push("world-settings: a minimum above its maximum, using the defaults".into());
            (min_sim, max_sim, min_view, max_view) = (4, 12, 6, 16);
        }
        let mode = match v.get("adjustment-mode").map(|s| s.to_ascii_lowercase()).as_deref() {
            None | Some("mixed") => Mode::Mixed,
            Some("proactive") => Mode::Proactive,
            Some("reactive") => Mode::Reactive,
            Some("off") => Mode::Off,
            Some(other) => {
                bad.push(format!("adjustment-mode: unknown mode {other}, using mixed"));
                Mode::Mixed
            }
        };
        let empty_world = match v
            .get("proactive-mode-settings.empty-world-target")
            .map(|s| s.to_ascii_lowercase())
            .as_deref()
        {
            None | Some("min") => EmptyTarget::Min,
            Some("max") => EmptyTarget::Max,
            Some("keep") => EmptyTarget::Keep,
            Some(other) => {
                bad.push(format!("empty-world-target: unknown value {other}, using min"));
                EmptyTarget::Min
            }
        };
        let profiles = v.profiles(&mut bad);
        let flag = |key: &str, default: bool| v.get(key).map_or(default, |s| s == "true");
        let cfg = Self {
            enabled: flag("enabled", true),
            mode,
            start_delay,
            ticks_per_check,
            passed_checks_for_increase: passed_up,
            passed_checks_for_decrease: passed_down,
            log_changes: flag("log-changes", true),
            announce: flag("announce-changes-in-chat", false),
            announce_format: v.get("announce-format").unwrap_or(DEFAULT_ANNOUNCE).to_owned(),
            notify_min_permission,
            command_min_permission,
            ticking_target,
            non_ticking_target,
            empty_world,
            increase_mspt,
            decrease_mspt,
            prioritize_simulation: flag(&format!("{r}prioritize-simulation-distance"), false),
            chunk_weight,
            max_increase_step,
            max_decrease_step,
            revert_increase_if_overloaded: flag(&format!("{r}revert-increase-if-overloaded"), true),
            prediction: flag(&format!("{pr}enabled"), true),
            prediction_history_minutes,
            prediction_min_samples,
            variance: flag(&format!("{va}enabled"), true),
            variance_threshold,
            variance_min_samples,
            variance_block_increase: flag(&format!("{va}block-increase-on-high-variance"), true),
            variance_min_avg,
            gen_safeguard: flag("chunk-generation-safeguard.enabled", true),
            gen_chunks_per_second,
            gen_lock_increase: flag("chunk-generation-safeguard.lock-increase-during-gen", true),
            gen_emergency_view,
            memory_safeguard: flag("memory-safeguard.enabled", true),
            memory_min_free_percent,
            grace: flag("startup-grace.enabled", false),
            grace_sim,
            grace_view,
            grace_ticks,
            afk: flag("afk-player-throttling.enabled", false),
            afk_minutes,
            afk_sim,
            afk_view,
            ping: flag("ping-throttling.enabled", false),
            ping_threshold_ms,
            ping_checks,
            ping_sim,
            ping_view,
            fixed_view: flag("per-player-fixed-view-distance.enabled", false),
            fixed_view_prefix,
            fixed_sim: flag("per-player-fixed-simulation-distance.enabled", false),
            fixed_sim_prefix,
            density: flag("density-based-scaling.enabled", false),
            cluster_radius,
            cluster_threshold,
            max_bonus,
            relief: flag("emergency-relief.enabled", false),
            relief_clear_weather: flag("emergency-relief.clear-weather", true),
            relief_freeze_time: flag("emergency-relief.freeze-time", true),
            relief_recover_mspt,
            diagnostics: flag("diagnostics.enabled", false),
            diag_interval_seconds,
            diag_extra_above_mspt,
            diag_record_samples: flag("diagnostics.record-samples", true),
            diag_record_changes: flag("diagnostics.record-changes", true),
            diag_max_file_mb,
            diag_keep_days,
            profiles,
            cap_view: None,
            cap_sim: None,
            min_sim,
            max_sim,
            min_view,
            max_view,
        };
        (cfg, bad)
    }

    /// This config with the profile `name` laid over it; None for an unknown name. "normal" is
    /// the config itself.
    pub fn with_profile(&self, name: &str) -> Option<Self> {
        if name.eq_ignore_ascii_case("normal") {
            return Some(self.clone());
        }
        let p = self.profiles.iter().find(|p| p.name.eq_ignore_ascii_case(name))?;
        let mut c = self.clone();
        let set = |field: &mut f64, value: Option<f64>| {
            if let Some(v) = value {
                *field = v;
            }
        };
        set(&mut c.decrease_mspt, p.decrease_mspt);
        set(&mut c.increase_mspt, p.increase_mspt);
        set(&mut c.ticking_target, p.ticking_target);
        set(&mut c.non_ticking_target, p.non_ticking_target);
        c.max_increase_step = p.max_increase_step.unwrap_or(c.max_increase_step);
        c.max_decrease_step = p.max_decrease_step.unwrap_or(c.max_decrease_step);
        c.passed_checks_for_increase = p.passed_checks_for_increase.unwrap_or(c.passed_checks_for_increase);
        c.passed_checks_for_decrease = p.passed_checks_for_decrease.unwrap_or(c.passed_checks_for_decrease);
        c.cap_view = p.cap_view;
        c.cap_sim = p.cap_sim;
        Some(c)
    }

    /// The profile names, "normal" first.
    pub fn profile_names(&self) -> Vec<String> {
        std::iter::once("normal".to_owned()).chain(self.profiles.iter().map(|p| p.name.clone())).collect()
    }
}

const DEFAULT_ANNOUNCE: &str = "&7[VDT] &f{world}: Sim &e{oldSim}&7\u{2192}&a{newSim} &f| View &e{oldView}&7\u{2192}&a{newView}";

/// `section.key` to the raw value text.
struct Values(HashMap<String, String>);

impl Values {
    fn read(text: &str) -> Self {
        let mut map = HashMap::new();
        let mut section = String::new();
        for line in text.lines() {
            let line = strip_comment(line).trim();
            if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                section = name.trim().to_owned();
            } else if let Some((key, value)) = line.split_once('=') {
                let value = value.trim().trim_matches('"').to_owned();
                let key = key.trim().trim_matches('"');
                let full = if section.is_empty() { key.to_owned() } else { format!("{section}.{key}") };
                map.insert(full, value);
            }
        }
        Self(map)
    }
    fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(String::as_str)
    }

    /// The `[profiles.<name>]` sections, sorted by name.
    fn profiles(&self, bad: &mut Vec<String>) -> Vec<Profile> {
        let mut out: Vec<Profile> = Vec::new();
        let mut keys: Vec<_> = self.0.iter().filter_map(|(k, v)| Some((k.strip_prefix("profiles.")?, v))).collect();
        keys.sort();
        for (rest, raw) in keys {
            let Some((name, key)) = rest.rsplit_once('.') else {
                continue;
            };
            if !out.last().is_some_and(|p| p.name == name) {
                out.push(Profile { name: name.to_owned(), ..Profile::default() });
            }
            let p = out.last_mut().expect("pushed above");
            let Ok(n) = raw.parse::<f64>() else {
                bad.push(format!("profiles.{name}.{key}: expected a number, got {raw}"));
                continue;
            };
            match key {
                "decrease-mspt-threshold" => p.decrease_mspt = Some(n),
                "increase-mspt-threshold" => p.increase_mspt = Some(n),
                "max-increase-step" => p.max_increase_step = Some(n.max(1.0) as i32),
                "max-decrease-step" => p.max_decrease_step = Some(n.max(1.0) as i32),
                "passed-checks-for-increase" => p.passed_checks_for_increase = Some(n.max(1.0) as u32),
                "passed-checks-for-decrease" => p.passed_checks_for_decrease = Some(n.max(1.0) as u32),
                "global-ticking-chunk-count-target" => p.ticking_target = Some(n),
                "global-non-ticking-chunk-count-target" => p.non_ticking_target = Some(n),
                "max-view-distance-cap" => p.cap_view = Some(n.max(2.0) as i32),
                "max-simulation-distance-cap" => p.cap_sim = Some(n.max(2.0) as i32),
                other => bad.push(format!("profiles.{name}: unknown key {other}")),
            }
        }
        out
    }
}

/// Drops a trailing `# comment` that is not inside a string.
fn strip_comment(line: &str) -> &str {
    let mut quoted = false;
    for (i, c) in line.char_indices() {
        match c {
            '"' => quoted = !quoted,
            '#' if !quoted => return &line[..i],
            _ => {}
        }
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_default_has_the_plugins_bundled_values_and_no_problems() {
        let (cfg, bad) = Config::parse(DEFAULT_TOML);
        assert!(bad.is_empty(), "{bad:?}");
        assert_eq!(cfg.mode, Mode::Mixed);
        assert_eq!((cfg.start_delay, cfg.ticks_per_check), (2400, 200));
        assert_eq!((cfg.passed_checks_for_increase, cfg.passed_checks_for_decrease), (9, 1));
        assert_eq!((cfg.ticking_target, cfg.non_ticking_target), (5780.0, 6720.0));
        assert_eq!((cfg.increase_mspt, cfg.decrease_mspt), (40.0, 47.0));
        assert_eq!((cfg.max_increase_step, cfg.max_decrease_step), (2, 4));
        assert_eq!((cfg.min_sim, cfg.max_sim, cfg.min_view, cfg.max_view), (4, 12, 6, 16));
        assert_eq!(cfg.chunk_weight, 1.0);
        assert_eq!((cfg.gen_chunks_per_second, cfg.gen_emergency_view), (8.0, 8));
        assert_eq!((cfg.grace_sim, cfg.grace_view, cfg.grace_ticks), (4, 6, 6000));
        assert!(!cfg.grace && !cfg.prioritize_simulation && cfg.gen_safeguard && cfg.memory_safeguard);
    }

    #[test]
    fn features_that_were_off_stay_off_and_profiles_load() {
        let (cfg, _) = Config::parse(DEFAULT_TOML);
        assert!(!cfg.announce && !cfg.afk && !cfg.ping && !cfg.density && !cfg.relief && !cfg.diagnostics);
        assert_eq!((cfg.afk_sim, cfg.afk_view, cfg.ping_sim, cfg.ping_view), (4, 6, 4, 6));
        assert_eq!((cfg.ping_threshold_ms, cfg.ping_checks), (200.0, 3));
        assert_eq!((cfg.cluster_radius, cfg.cluster_threshold, cfg.max_bonus), (8, 5, 4));
        assert_eq!(cfg.relief_recover_mspt, 40.0);
        assert_eq!(cfg.profile_names(), ["normal", "event-mode", "night-mode"]);
        let event = cfg.with_profile("event-mode").unwrap();
        assert_eq!((event.decrease_mspt, event.cap_view, event.passed_checks_for_increase), (42.0, Some(8), 60));
        assert_eq!(event.increase_mspt, 40.0);
        let night = cfg.with_profile("NIGHT-MODE").unwrap();
        assert_eq!((night.increase_mspt, night.max_increase_step, night.cap_view), (45.0, 2, None));
        assert_eq!(cfg.with_profile("normal").unwrap(), cfg);
        assert!(cfg.with_profile("party").is_none());
        assert!(cfg.announce_format.contains("{oldSim}"));
    }

    #[test]
    fn an_unknown_profile_key_is_reported() {
        let (cfg, bad) = Config::parse("[profiles.x]\nmax-view-distance-cap = 9\nwarp = 2\n");
        assert_eq!(cfg.profiles.len(), 1);
        assert_eq!(cfg.profiles[0].cap_view, Some(9));
        assert_eq!(bad.len(), 1);
    }

    #[test]
    fn an_empty_file_is_the_default() {
        // The shipped file adds two example profiles; everything else is the built-in default.
        let shipped = Config { profiles: Vec::new(), ..Config::parse(DEFAULT_TOML).0 };
        assert_eq!(Config::parse("").0, shipped);
    }

    #[test]
    fn a_bad_value_is_reported_and_replaced() {
        let (cfg, bad) = Config::parse("ticks-per-check = 0\nadjustment-mode = \"sideways\"\n");
        assert_eq!(cfg.ticks_per_check, 200);
        assert_eq!(cfg.mode, Mode::Mixed);
        assert_eq!(bad.len(), 2);
    }

    #[test]
    fn comments_and_sections_are_read() {
        let (cfg, _) = Config::parse("adjustment-mode = \"off\" # none\n[world-settings.default.view-distance]\nmax-view-distance = 14\n");
        assert_eq!(cfg.mode, Mode::Off);
        assert_eq!(cfg.max_view, 14);
    }
}
