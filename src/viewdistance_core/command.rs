//! `/vdt`: the answers, as `&`-coded lines. The texts are the original's English defaults; the
//! server's one world is called "world".
use crate::viewdistance_core::engine::{std_dev, Distances, Engine, Observation};
use crate::viewdistance_core::messages::t;

/// The usages registered with the server (completion and operator level): one per real sub-command
/// path, with literals for the fixed words and typed arguments for the rest. `--duration` and the
/// `--fixed`/`--weight` flags are literal words (the flags in either order); a usage with a required
/// argument after an optional one is refused by the server, so the optional world comes before it
/// only where it is the last argument. The first usage takes any other word after `vdt` (the original
/// answers an unknown sub-command with the help); distance values are words so a bad one reaches
/// the original's "Invalid value" answer.
pub const USAGES: &[&str] = &[
    "vdt [rest:text]",
    "vdt help",
    "vdt reload",
    "vdt enable",
    "vdt disable",
    "vdt status",
    "vdt status --fixed",
    "vdt status --weight",
    "vdt status --fixed --weight",
    "vdt status --weight --fixed",
    "vdt history",
    "vdt simulate [world:word]",
    "vdt mspt",
    "vdt mspt debug",
    "vdt profile [name:word]",
    "vdt viewdistance",
    "vdt viewdistance <value:word>",
    "vdt viewdistance <value:word> <world:word>",
    "vdt viewdistance <value:word> --duration <minutes:word>",
    "vdt viewdistance <value:word> <world:word> --duration <minutes:word>",
    "vdt simulationdistance",
    "vdt simulationdistance <value:word>",
    "vdt simulationdistance <value:word> <world:word>",
    "vdt simulationdistance <value:word> --duration <minutes:word>",
    "vdt simulationdistance <value:word> <world:word> --duration <minutes:word>",
    "vdt diagnose",
    "vdt diagnose on",
    "vdt diagnose off",
    "vdt diagnose status",
    "vdt diagnose note",
    "vdt diagnose note <text:text>",
    "vdt analyze",
    "vdt analyse",
    "vdt doctor",
    "vdt preset",
    "vdt preset <name:word>",
];

/// Every group node (declared with the others at gather time), with the root node first.
pub const SUB_NODES: [&str; 8] = [
    "viewdistancetweaks.vdt.command.reload",
    "viewdistancetweaks.vdt.command.status",
    "viewdistancetweaks.vdt.command.simulate",
    "viewdistancetweaks.vdt.command.profile",
    "viewdistancetweaks.vdt.command.viewdistance",
    "viewdistancetweaks.vdt.command.simulationdistance",
    "viewdistancetweaks.vdt.command.preset",
    "viewdistancetweaks.vdt.command.pregen",
];

/// The group node a `/vdt` line needs besides the root node (without the slash): the node of the
/// sub-command word, as the original's `onCommand` switch maps it. None for the bare command, unknown
/// words and help, which need the root node only; None when the line is not `/vdt`.
pub fn node_of(line: &str) -> Option<&'static str> {
    let words: Vec<&str> = line.split_whitespace().collect();
    if !words.first()?.eq_ignore_ascii_case("vdt") {
        return None;
    }
    let sub = words.get(1)?.to_ascii_lowercase();
    Some(match sub.as_str() {
        "reload" | "enable" | "disable" | "diagnose" | "update" => SUB_NODES[0],
        "status" | "history" | "mspt" | "doctor" | "analyze" | "analyse" => SUB_NODES[1],
        "simulate" => SUB_NODES[2],
        "profile" => SUB_NODES[3],
        "viewdistance" => SUB_NODES[4],
        "simulationdistance" => SUB_NODES[5],
        "preset" => SUB_NODES[6],
        "pregen" => SUB_NODES[7],
        _ => return None,
    })
}

/// The permission node of the in-chat notices of distance changes.
pub const NOTIFY_NODE: &str = "viewdistancetweaks.notify";

/// Where diagnostics are written, inside the world's folder (shown to operators).
pub const DIAG_FOLDER: &str = "viewdistance/diagnostics";

/// The completions for the word being typed: `args` are the words after `vdt`, the last one the
/// (possibly empty) start of the word being typed; `profiles` the profile names, "normal" first. The
/// original's `onTabComplete`, for the sub-commands this port has.
pub fn complete(args: &[&str], profiles: &[String]) -> Vec<String> {
    let start = |input: &str, options: &[&str]| -> Vec<String> {
        options.iter().filter(|o| o.to_lowercase().starts_with(&input.to_lowercase())).map(|o| (*o).to_string()).collect()
    };
    let sub = args.first().map(|a| a.to_ascii_lowercase()).unwrap_or_default();
    match (args.len(), sub.as_str()) {
        (1, _) => start(
            args[0],
            &[
                "reload", "status", "history", "analyze", "doctor", "diagnose", "simulate", "simulationdistance", "viewdistance",
                "enable", "disable", "mspt", "preset", "profile",
            ],
        ),
        (2, "preset") => start(args[1], PRESET_NAMES),
        (2, "profile") => {
            let names: Vec<&str> = profiles.iter().map(String::as_str).collect();
            start(args[1], &names)
        }
        (2, "simulationdistance" | "viewdistance") => start(args[1], &["2", "4", "6", "8", "10", "12", "16"]),
        (3, "simulationdistance" | "viewdistance") => start(args[2], &["world", "--duration"]),
        (2, "status") => start(args[1], &["--weight", "--fixed"]),
        (2, "diagnose") => start(args[1], &["on", "off", "status", "note"]),
        (2, "simulate") => start(args[1], &["world"]),
        _ => Vec::new(),
    }
}

/// What the game does after the lines are shown.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Reply {
    pub lines: Vec<String>,
    /// Read `config/viewdistance.toml` again (`/vdt reload`).
    pub reload: bool,
    /// Switch diagnostics on or off until the next restart, or write a note.
    pub diagnose: Option<Diagnose>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Diagnose {
    On,
    Off,
    Status,
    Note(String),
}

fn reply(lines: Vec<String>) -> Option<Reply> {
    let prefix = crate::viewdistance_core::messages::prefix();
    let lines = if prefix.is_empty() { lines } else { lines.into_iter().map(|l| format!("{prefix}{l}")).collect() };
    Some(Reply { lines, ..Reply::default() })
}

fn f2(v: f64) -> String {
    format!("{v:.2}")
}

fn tps(mspt: f64) -> f64 {
    if mspt <= 0.0 {
        20.0
    } else {
        (1000.0 / mspt).min(20.0)
    }
}

/// The help lines of messages.yml (`help:`), for the sub-commands this port has. The lines for
/// pregen and update are left out: those commands are not ported (see HMOD_PORT.md).
pub fn help() -> Vec<String> {
    [
        "&6--- VDT Commands ---",
        "&e/vdt reload                              &7- Reload configuration",
        "&e/vdt status [--weight|--fixed]           &7- Show status",
        "&e/vdt simulate [world]                    &7- Preview next check without applying it",
        "&e/vdt history                             &7- Show recent distance changes",
        "&e/vdt analyze                             &7- Suggestions from this server's own history",
        "&e/vdt doctor                              &7- Check setup, conflicts and updates in one go",
        "&e/vdt diagnose [on|off|status|note <text>] &7- Record what the plugin sees to a log file",
        "&e/vdt simulationdistance <value> [world] [--duration <min>]",
        "&e/vdt viewdistance <value> [world] [--duration <min>]",
        "&e/vdt enable / disable                    &7- Toggle the plugin",
        "&e/vdt mspt                                &7- Show MSPT, TPS & variance",
        "&e/vdt preset <small-server|large-network|performance-first>",
        "&e/vdt profile [name]                      &7- Show or switch the active profile",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

/// Runs `line` (without its slash) when it is `/vdt`; None otherwise. `actual` is what the
/// server runs at, `o` what it looks like now (only the tick times and players matter here).
pub fn run(e: &mut Engine, line: &str, actual: Distances, o: &Observation) -> Option<Reply> {
    let mut words = line.split_whitespace();
    let label = words.next()?.to_ascii_lowercase();
    if !matches!(label.as_str(), "vdt" | "viewdistancetweaks" | "viewdistancetweaks:vdt") {
        return None;
    }
    let sub = words.next().map(str::to_ascii_lowercase);
    let rest: Vec<&str> = words.collect();
    match sub.as_deref() {
        Some("reload") => Some(Reply { reload: true, ..Reply::default() }),
        Some("enable") => {
            e.switch(true);
            reply(vec![t("enabled", "&aPlugin enabled.", &[])])
        }
        Some("disable") => {
            e.switch(false);
            reply(vec![t("disabled", "&cPlugin disabled.", &[])])
        }
        Some("status") => reply(status(e, actual, o, rest.contains(&"--fixed"), rest.contains(&"--weight"))),
        Some("mspt") => reply(mspt(e, o, rest.first() == Some(&"debug"))),
        Some("history") => reply(history(e)),
        Some("simulate") => reply(simulate(e, actual, o, rest.first().copied())),
        Some("profile") => reply(profile(e, rest.first().copied())),
        Some("preset") => reply(preset(e, rest.first().copied())),
        Some("pregen") => reply(vec!["&cPre-generation is not available on this server: Horizon has no chunk-generation API yet.".into()]),
        Some("update") => reply(vec!["&7Update checks are not part of this port; the mod is updated with the server.".into()]),
        Some("analyze" | "analyse") => reply(analyze(e, o)),
        Some("doctor") => reply(doctor(e, o)),
        Some(which @ ("viewdistance" | "simulationdistance")) => reply(pin(e, which == "simulationdistance", &rest, actual)),
        Some("diagnose") => {
            let d = match rest.first().map(|w| w.to_ascii_lowercase()).as_deref() {
                Some("on") => Diagnose::On,
                Some("off") => Diagnose::Off,
                Some("note") if rest.len() > 1 => Diagnose::Note(rest[1..].join(" ")),
                Some("note") => return reply(vec![t("diagnose.note-usage", "&cUsage: /vdt diagnose note <text>", &[])]),
                // Anything else (none, "status") shows the state, as the original does.
                _ => Diagnose::Status,
            };
            // The answer depends on whether diagnostics run, which only the caller knows (`diagnose`).
            Some(Reply { diagnose: Some(d), ..Reply::default() })
        }
        _ => reply(help()),
    }
}

fn status(e: &Engine, d: Distances, o: &Observation, fixed: bool, weight: bool) -> Vec<String> {
    let c = e.config();
    if fixed {
        let mut lines = vec![t("status.fixed.header", "&6=== Active per-player overrides ===", &[])];
        if !c.afk && !c.ping && !c.fixed_view && !c.fixed_sim {
            lines.push(
                "&7No per-player distance feature is enabled in config.yml &8(per-player-fixed-view-distance, \
                 per-player-fixed-simulation-distance, afk-player-throttling, ping-throttling)."
                    .into(),
            );
            return lines;
        }
        let throttled = e.throttled();
        if throttled.is_empty() {
            lines.push(t("status.fixed.none", "&7No player currently has a fixed view or simulation distance.", &[]));
        }
        for (id, p, by) in throttled {
            let source = match by {
                "PING" => "&b[PING]",
                "FIXED" => "&8[permission]",
                _ => "&6[AFK]",
            };
            lines.push(format!("&e{id:032x} &7in &fworld"));
            lines.push(format!("  &7View: &a{} &8(world: {}) {source}", p.view, d.view));
            lines.push(format!("  &7Sim:  &a{} &8(world: {}) {source}", p.sim, d.sim));
        }
        lines.push(format!(
            "&8Grant &7{}<2-32> &8or &7{}<2-32>&8; highest value wins.",
            c.fixed_view_prefix, c.fixed_sim_prefix
        ));
        return lines;
    }
    let mode = format!("{:?}", c.mode).to_ascii_lowercase();
    let mut lines = vec![
        t("status.header", "&6=== ViewDistanceTweaks Status ===", &[]),
        t("status.summary", "&7Mode: &f{mode} &7| MSPT: &f{mspt} &7| TPS: &f{tps} &7| Active: &f{active}", &[("mode", mode), ("mspt", f2(o.mspt)), ("tps", f2(o.tps.unwrap_or_else(|| tps(o.mspt)))), ("active", e.active().to_string())]),
    ];
    if !c.variance {
        // The original shows no variance line while `mspt-variance` is off.
    } else if o.samples_ms.len() >= c.variance_min_samples {
        let sd = std_dev(&o.samples_ms);
        // Same three tiers as the original (status.variance): red above the threshold, yellow above
        // 60% of it, green otherwise; the warning only when it is above the threshold.
        let (colour, warn) = if sd > c.variance_threshold {
            ("&c", " &c\u{26a0} unstable")
        } else if sd > c.variance_threshold * 0.6 {
            ("&e", "")
        } else {
            ("&a", "")
        };
        lines.push(format!(
            "&7MSPT variance (stddev): {colour}{}ms &7(threshold: &f{}ms&7){warn}",
            f2(sd),
            f2(c.variance_threshold)
        ));
    } else {
        lines.push(t("status.variance-no-samples", "&7MSPT variance: &8not enough samples yet", &[]));
    }
    if let Some(left) = e.grace_left() {
        lines.push(format!(
            "&e\u{23f3} Startup grace active: &f{}s remaining &7(sim={}, view={})",
            left / 20,
            c.grace_sim,
            c.grace_view
        ));
    }
    let players = f64::from(o.players);
    let sq = |v: i32| f64::from(2 * v + 1).powi(2);
    let mut world = format!(
        "&eworld &7| Sim: &f{} &7| View: &f{} &7| Ticking: &f{} &7| Non-Ticking: &f{}",
        d.sim,
        d.view,
        sq(d.sim) * players,
        (sq(d.view) - sq(d.sim)) * players,
    );
    if weight {
        world += &format!(" &7| Weight: &f{:?}", c.chunk_weight);
    }
    world += &format!(" &7| Next check: &f{}s", e.next_check_in() / 20);
    lines.push(world);
    // The original shows the profile line only while a profile other than "normal" is active.
    if e.profile() != "normal" {
        lines.push(t("status.profile", "&7Active profile: &b{profile}", &[("profile", e.profile().to_string())]));
    }
    if let Some(free) = o.free_memory_percent.filter(|_| c.memory_safeguard) {
        let low = free < c.memory_min_free_percent;
        // The original adds "(used MB live / max MB)" here; the port has no heap figures (GAPS.md).
        lines.push(format!(
            "&7Free heap: {}{}% &7(min {}%){}",
            if low { "&c" } else { "&a" },
            f2(free),
            f2(c.memory_min_free_percent),
            if low { " &c\u{26a0} increases blocked" } else { "" }
        ));
    }
    let throttled = e.throttled();
    if c.afk {
        let n = throttled.iter().filter(|t| t.2 == "AFK").count();
        lines.push(format!("&7AFK players throttled: &f{n} &8(via idle timer)"));
    }
    if c.ping {
        let n = throttled.iter().filter(|t| t.2 == "PING").count();
        lines.push(format!("&7High-ping players throttled: &f{n} &8(threshold {}ms)", c.ping_threshold_ms));
    }
    if e.relief_active() {
        lines.push("&c\u{26a0} Emergency relief active in: &fworld".into());
    }
    lines
}

fn mspt(e: &Engine, o: &Observation, debug: bool) -> Vec<String> {
    // Same layout as the original: the sample count, min/max and the last ten values appear only
    // under "mspt debug" (the original's debug branch); the samples are oldest first.
    let s = &o.samples_ms;
    let mut lines = vec![
        format!("&7MSPT (avg): &f{}", f2(o.mspt)),
        format!("&7TPS:        &f{}", f2(o.tps.unwrap_or_else(|| tps(o.mspt)))),
    ];
    if e.config().variance {
        lines.push(format!("&7MSPT stddev: &f{} &7(threshold: &f{:.1}&7)", f2(std_dev(s)), e.config().variance_threshold));
    }
    if debug {
        lines.push(format!("&7Samples: &f{} &7(source: the server's tick times)", s.len()));
        if !s.is_empty() {
            let min = s.iter().copied().fold(f64::INFINITY, f64::min);
            let max = s.iter().copied().fold(0.0, f64::max);
            lines.push(format!("&7Min: &f{} &7Max: &f{}", f2(min), f2(max)));
            let last: Vec<String> = s[s.len().saturating_sub(10)..].iter().map(|v| format!("{v:.1}")).collect();
            lines.push(format!("&7Last 10: &f{}", last.join(" ")));
        }
    }
    lines
}

/// The original's formatAgo: seconds, minutes, then hours and minutes, then days and hours.
fn ago(ticks: u64) -> String {
    let secs = ticks / 20;
    let minutes = secs / 60;
    let hours = minutes / 60;
    match secs {
        0..60 => format!("{secs}s ago"),
        _ if minutes < 60 => format!("{minutes}m ago"),
        _ if hours < 24 => format!("{hours}h {}m ago", minutes % 60),
        _ => format!("{}d {}h ago", hours / 24, hours % 24),
    }
}

fn history(e: &Engine) -> Vec<String> {
    let entries: Vec<_> = e.history().collect();
    if entries.is_empty() {
        return vec![t("history.empty", "&7No distance changes recorded yet since the last restart.", &[])];
    }
    let mut lines = vec![t("history.header", "&6=== Last {count} distance changes ===", &[("count", entries.len().to_string())])];
    for (tick, c) in entries.into_iter().rev() {
        lines.push(t(
            "history.entry",
            "&8{ago} &e{world} &7Sim: &f{oldSim}&7\u{2192}&a{newSim} &7View: &f{oldView}&7\u{2192}&a{newView}",
            &[("ago", ago(e.ticks() - tick)), ("world", "world".into()), ("oldSim", c.from.sim.to_string()), ("newSim", c.to.sim.to_string()), ("oldView", c.from.view.to_string()), ("newView", c.to.view.to_string())],
        ));
        lines.push(t("history.reason", "  &8{reason}", &[("reason", c.reason.to_string())]));
    }
    lines
}

fn simulate(e: &Engine, cur: Distances, o: &Observation, world: Option<&str>) -> Vec<String> {
    // The original checks "enabled" before the world name, so a disabled plugin answers first.
    if !e.active() {
        return vec![t("simulate.disabled", "&cPlugin is disabled - enable it first (/vdt enable) to simulate a check.", &[])];
    }
    // The server's one world is called "world" (see the module comment).
    if let Some(w) = world.filter(|w| *w != "world") {
        return vec![t("world-not-found", "&cWorld not found: {world}", &[("world", (*w).to_string())])];
    }
    let (to, reason, now) = e.simulate(cur, o);
    // Original colouring (simulate.entry): the arrow is green when the next check would apply and
    // grey when not; the new values are yellow when anything changes and grey when nothing does.
    let arrow = if now { "&a\u{2192}" } else { "&8\u{2192}" };
    let colour = if to == cur { "&7" } else { "&e" };
    let verdict = if now && to != cur { t("simulate.would-apply", " &a[would apply]", &[]) } else { t("simulate.would-not-apply", " &8[not yet / no change]", &[]) };
    vec![
        t("simulate.header", "&6=== /vdt simulate {world} ===", &[("world", world.unwrap_or("(all worlds)").to_string())]),
        t("simulate.note", "&7This shows what the NEXT check would compute. Nothing is applied.", &[]),
        t("simulate.mspt", "&7Current MSPT: &f{mspt} &7| stddev: &f{stddev}", &[("mspt", f2(o.mspt)), ("stddev", f2(std_dev(&o.samples_ms)))]),
        t(
            "simulate.entry",
            "&e{world} &7| Sim: &f{currentSim} {arrow} {color}{targetSim} &7| View: &f{currentView} {arrow} {color}{targetView}",
            &[("world", "world".into()), ("currentSim", cur.sim.to_string()), ("arrow", arrow.to_string()), ("color", colour.to_string()), ("targetSim", to.sim.to_string()), ("currentView", cur.view.to_string()), ("targetView", to.view.to_string())],
        ),
        t("simulate.reason", "  &8{reason}{verdict}", &[("reason", reason.to_string()), ("verdict", verdict)]),
    ]
}

fn profile(e: &mut Engine, name: Option<&str>) -> Vec<String> {
    let names = e.base_config().profile_names().join(", ");
    match name {
        None => vec![t("profile.current", "&7Active profile: &b{profile}", &[("profile", e.profile().to_string())]), t("profile.available", "&7Available: &f{profiles}", &[("profiles", names)])],
        Some(n) if e.set_profile(n) => vec![t("profile.applied", "&aSwitched to profile &f{profile}&a.", &[("profile", e.profile().to_string())])],
        Some(n) => vec![t("profile.unknown", "&cUnknown profile: {profile} &7(available: {profiles})", &[("profile", n.to_string()), ("profiles", names)])],
    }
}

/// `/vdt viewdistance|simulationdistance <value> [world] [--duration <min>]`.
fn pin(e: &mut Engine, sim: bool, args: &[&str], cur: Distances) -> Vec<String> {
    let usage = if sim {
        t("override.usage-sim", "&cUsage: /vdt simulationdistance <value> [world] [--duration <min>]", &[])
    } else {
        t("override.usage-view", "&cUsage: /vdt viewdistance <value> [world] [--duration <min>]", &[])
    };
    let Some(raw) = args.first() else {
        return vec![usage];
    };
    // Not a number counts as -1, so it is out of range, and the answer shows the raw text.
    let value = raw.parse::<i32>().unwrap_or(-1);
    if !(2..=32).contains(&value) {
        return vec![t("invalid-range", "&cInvalid value: &f{value}&c. Must be between &f{min}&c and &f{max}&c.", &[("value", (*raw).to_string()), ("min", "2".into()), ("max", "32".into())])];
    }
    // The original's parseInt(..., 0): a duration that is not a positive number means "permanent".
    let minutes = args
        .iter()
        .position(|a| *a == "--duration")
        .and_then(|i| args.get(i + 1))
        .and_then(|m| m.parse::<u64>().ok())
        .filter(|m| *m > 0);
    // The optional world (second word, unless it is a flag). The one world is called "world".
    if let Some(w) = args.get(1).filter(|w| !w.starts_with("--")).filter(|w| **w != "world") {
        return vec![t("world-not-found", "&cWorld not found: {world}", &[("world", (*w).to_string())])];
    }
    let mut lines = Vec::new();
    let mut value = value;
    if !sim && value < cur.sim {
        lines.push(t(
            "override.view-below-sim",
            "&e{world}: requested view distance {view} is below simulation distance {sim} - using {sim} instead (view must be >= sim).",
            &[("world", "world".into()), ("view", value.to_string()), ("sim", cur.sim.to_string())],
        ));
        value = cur.sim;
    }
    // Simulation distance: the view is raised to it, with the same message and the same duration,
    // as the original does when it applies the pair.
    let mut view_pin = None;
    if sim && cur.view < value {
        lines.push(t(
            "override.view-below-sim",
            "&e{world}: requested view distance {view} is below simulation distance {sim} - using {sim} instead (view must be >= sim).",
            &[("world", "world".into()), ("view", cur.view.to_string()), ("sim", value.to_string())],
        ));
        view_pin = Some(value);
    }
    e.pin(sim, value, minutes);
    if let Some(view) = view_pin {
        e.pin(false, view, minutes);
    }
    let label = if sim { t("override.label-sim", "Simulation distance", &[]) } else { t("override.label-view", "View distance", &[]) };
    lines.push(match minutes {
        Some(m) => t("override.applied-temporary", "&a{label} set for {minutes} min.", &[("label", label), ("minutes", m.to_string())]),
        None => t("override.applied-permanent", "&a{label} set (permanent until reload).", &[("label", label)]),
    });
    lines
}

/// The preset names (`ConfigPresets.PRESETS`).
pub const PRESET_NAMES: &[&str] = &["small-server", "large-network", "performance-first"];

/// `ConfigPresets`: the values each preset sets, applied to the config in force. Returns how many values the original's
/// preset holds (its `size()`), which is what the answer reports.
fn apply_preset(c: &mut crate::viewdistance_core::config::Config, name: &str) -> Option<usize> {
    use crate::viewdistance_core::config::Mode;
    match name {
        "small-server" => {
            c.mode = Mode::Mixed;
            c.increase_mspt = 35.0;
            c.decrease_mspt = 45.0;
            c.ticking_target = 8000.0;
            c.non_ticking_target = 9000.0;
            c.max_sim = 12;
            c.max_view = 20;
            Some(8)
        }
        "large-network" => {
            c.mode = Mode::Mixed;
            c.increase_mspt = 38.0;
            c.decrease_mspt = 44.0;
            c.max_decrease_step = 2;
            c.prioritize_simulation = true;
            c.ticking_target = 4000.0;
            c.non_ticking_target = 5000.0;
            c.max_sim = 8;
            c.max_view = 12;
            Some(9)
        }
        "performance-first" => {
            c.mode = Mode::Reactive;
            c.increase_mspt = 35.0;
            c.decrease_mspt = 42.0;
            c.max_decrease_step = 3;
            c.prioritize_simulation = true;
            c.variance_threshold = 6.0;
            c.max_sim = 6;
            c.max_view = 8;
            Some(8)
        }
        _ => None,
    }
}

/// `/vdt preset <name>`. The original rewrites config.yml; this port changes the settings in force, and they hold
/// until the next `/vdt reload` (the config file is the server's own and is not rewritten).
fn preset(e: &mut Engine, name: Option<&str>) -> Vec<String> {
    let all = PRESET_NAMES.join(", ");
    let Some(name) = name else {
        return vec![t("preset.usage", "&cUsage: /vdt preset <{presets}>", &[("presets", PRESET_NAMES.join("|"))])];
    };
    let lower = name.to_ascii_lowercase();
    let mut cfg = e.base_config().clone();
    let Some(count) = apply_preset(&mut cfg, &lower) else {
        return vec![t("preset.unknown", "&cUnknown preset: {preset} &7(available: {presets})", &[("preset", name.to_string()), ("presets", all)])];
    };
    e.set_config(cfg);
    vec![t("preset.applied", "&aApplied preset &f{preset}&a ({count} values changed). Config reloaded - fine-tune further as needed.", &[("preset", lower), ("count", count.to_string())])]
}

/// One `/vdt analyze` finding: severity (`problem`, `suggestion`, `fine`), option (`-` for none), suggestion, because.
struct Finding {
    severity: &'static str,
    option: &'static str,
    suggestion: String,
    because: String,
}

/// `ConfigAnalyzer.analyse` over what the engine has seen: the last 100 tick times (the chunk count is the one loaded now,
/// the port keeps no chunk history) and the recorded changes.
fn findings(e: &Engine, o: &Observation) -> Vec<Finding> {
    let c = e.config();
    let samples = &o.samples_ms;
    if samples.len() < 10 {
        return vec![Finding {
            severity: "fine",
            option: "-",
            suggestion: "Not enough data yet - leave it running for an hour or so.".into(),
            because: format!("Only {} MSPT samples so far; 10 are needed before any of this would mean anything.", samples.len()),
        }];
    }
    let peak = samples.iter().copied().fold(0.0, f64::max);
    let mean = samples.iter().sum::<f64>() / samples.len() as f64;
    let peak_chunks = f64::from(o.loaded_chunks);
    let mut out = Vec::new();
    if peak < c.increase_mspt * 0.5 {
        out.push(Finding {
            severity: "suggestion",
            option: "world-settings.<world>.max-view-distance",
            suggestion: "Raise the per-world maximums - this server has room to spare.".into(),
            because: format!("Peak MSPT was {peak:.1}ms against an increase threshold of {:.1}ms. It never came close to being busy, so the ceiling is what is limiting view distance, not performance.", c.increase_mspt),
        });
    }
    let gap = c.decrease_mspt - c.increase_mspt;
    if gap < 5.0 {
        let suggested = (c.decrease_mspt * 0.85).max(1.0);
        out.push(Finding {
            severity: "problem",
            option: "reactive-mode-settings.increase-mspt-threshold",
            suggestion: format!("Widen the gap - try {suggested:.0}, keeping decrease at {:.0}.", c.decrease_mspt),
            because: format!("Only {gap:.1}ms separates the two thresholds. That is the dead zone the distance is meant to settle in; too narrow and it steps up and down forever."),
        });
    }
    if mean > c.decrease_mspt {
        out.push(Finding {
            severity: "problem",
            option: "-",
            suggestion: "This server is over its decrease threshold on average, not just at peaks.".into(),
            because: format!("Mean MSPT {mean:.1}ms is above the {:.1}ms threshold. The plugin will hold distances at the minimum and cannot fix the underlying load - look for the cause first.", c.decrease_mspt),
        });
    }
    let history: Vec<_> = e.history().map(|(_, ch)| ch).collect();
    if history.len() >= 6 {
        let mut flips = 0;
        let mut previous: Option<bool> = None;
        for ch in history.iter().rev() {
            let up = ch.to.view > ch.from.view || ch.to.sim > ch.from.sim;
            if previous.is_some_and(|p| p != up) {
                flips += 1;
            }
            previous = Some(up);
        }
        if flips >= history.len() / 2 {
            out.push(Finding {
                severity: "problem",
                option: "reactive-mode-settings.revert-increase-if-overloaded",
                suggestion: "Turn this on, and widen the threshold gap.".into(),
                because: format!("{flips} of the last {} changes reversed direction. The distance is hunting rather than settling.", history.len()),
            });
        }
    }
    let seconds_per_step = (c.ticks_per_check as f64 / 20.0) * f64::from(c.passed_checks_for_increase);
    let seconds_per_chunk = seconds_per_step / f64::from(c.max_increase_step.max(1));
    if seconds_per_chunk > 120.0 {
        out.push(Finding {
            severity: "suggestion",
            option: "passed-checks-for-increase",
            suggestion: format!("Lower to {}, or raise max-increase-step to 2.", (c.passed_checks_for_increase / 2).max(2)),
            because: format!("At {} checks of {:.0}s and {} chunk(s) a step, growing one chunk takes {:.1} minutes. Climbing from minimum to maximum would take most of an hour of uninterrupted good health.", c.passed_checks_for_increase, c.ticks_per_check as f64 / 20.0, c.max_increase_step, seconds_per_chunk / 60.0),
        });
    }
    if c.max_decrease_step < c.max_increase_step {
        out.push(Finding {
            severity: "problem",
            option: "reactive-mode-settings.max-decrease-step",
            suggestion: "Raise it to at least max-increase-step.".into(),
            because: format!("Distance grows {} at a time but shrinks only {}. Under load it cannot give back what it took as fast as it took it.", c.max_increase_step, c.max_decrease_step),
        });
    }
    if matches!(c.mode, crate::viewdistance_core::config::Mode::Mixed | crate::viewdistance_core::config::Mode::Proactive) {
        let target = c.ticking_target + c.non_ticking_target;
        if target > 0.0 {
            let used = peak_chunks / target;
            if used > 0.95 && peak < c.increase_mspt {
                out.push(Finding {
                    severity: "suggestion",
                    option: "proactive-mode-settings.global-ticking-chunk-count-target",
                    suggestion: format!("Raise the targets - try {} ticking.", (c.ticking_target * 1.3) as i64),
                    because: format!("Chunk count reached {:.0}% of the budget while peak MSPT stayed at {peak:.1}ms, below the {:.1}ms increase threshold. The budget is the ceiling here, not performance.", used * 100.0, c.increase_mspt),
                });
            } else if used < 0.4 && peak > c.decrease_mspt {
                out.push(Finding {
                    severity: "suggestion",
                    option: "proactive-mode-settings.global-ticking-chunk-count-target",
                    suggestion: format!("Lower the targets - try {} ticking.", (c.ticking_target * 0.7) as i64),
                    because: format!("MSPT peaked at {peak:.1}ms while only {:.0}% of the chunk budget was in use. The budget is set higher than this hardware can carry, so MSPT has to do the braking alone.", used * 100.0),
                });
            }
        }
    }
    if out.is_empty() {
        out.push(Finding {
            severity: "fine",
            option: "-",
            suggestion: "Nothing worth changing.".into(),
            because: format!("Mean MSPT {mean:.1}, peak {peak:.1} over {} samples, and the distances are not fighting themselves.", samples.len()),
        });
    }
    out
}

/// `/vdt analyze`
fn analyze(e: &Engine, o: &Observation) -> Vec<String> {
    let mut lines = vec![t("analyze.header", "&6--- Config analysis --- &7(suggestions only, nothing is changed)", &[])];
    for f in findings(e, o) {
        let named = f.option != "-";
        let (key, default) = match (f.severity, named) {
            ("problem", true) => ("analyze.problem", "&c! &f{option} &7- &f{suggestion}"),
            ("problem", false) => ("analyze.problem-general", "&c! &f{suggestion}"),
            ("suggestion", true) => ("analyze.suggestion", "&e~ &f{option} &7- &f{suggestion}"),
            ("suggestion", false) => ("analyze.suggestion-general", "&e~ &f{suggestion}"),
            _ => ("analyze.fine", "&a+ &f{suggestion}"),
        };
        lines.push(t(key, default, &[("option", f.option.to_string()), ("suggestion", f.suggestion)]));
        lines.push(t("analyze.because", "&8    {because}", &[("because", f.because)]));
    }
    lines.push(t("analyze.footer", "&7Apply anything you agree with in config.yml, then &f/vdt reload&7.", &[]));
    lines
}

/// `/vdt doctor`: the checks of the original that apply here (thresholds, limits, memory, enabled).
fn doctor(e: &Engine, o: &Observation) -> Vec<String> {
    let c = e.config();
    let mut checks: Vec<(&str, String)> = Vec::new();
    if !e.active() {
        checks.push(("warn", "The plugin is disabled (enabled: false or /vdt disable). Nothing is adjusted.".into()));
    }
    if c.increase_mspt >= c.decrease_mspt {
        checks.push(("problem", format!("increase-mspt-threshold ({}) is not below decrease-mspt-threshold ({}). Distances will flip up and down. Keep a gap of at least 5 ms.", c.increase_mspt, c.decrease_mspt)));
    } else {
        checks.push(("ok", format!("MSPT thresholds: up below {} ms, down above {} ms.", c.increase_mspt, c.decrease_mspt)));
    }
    if c.min_sim > c.max_sim || c.min_view > c.max_view {
        checks.push(("problem", format!("World 'world' has a minimum above its maximum (sim {}-{}, view {}-{}).", c.min_sim, c.max_sim, c.min_view, c.max_view)));
    }
    if let Some(free) = o.free_memory_percent {
        checks.push((if free < 15.0 { "warn" } else { "ok" }, format!("Free memory: {free:.0}%{}", if free < 15.0 { ". Increases are blocked until more is free." } else { "." })));
    }
    checks.push(("ok", "PlaceholderAPI: not used by this port (see the README)".into()));
    let mut lines = vec![t("doctor.header", "&6--- ViewDistanceTweaks doctor --- &7(reads only, changes nothing)", &[])];
    let mut problems = 0;
    for (kind, text) in checks {
        if kind == "problem" {
            problems += 1;
        }
        let default = match kind {
            "ok" => "&a+ &f{text}",
            "warn" => "&e~ &f{text}",
            _ => "&c! &f{text}",
        };
        lines.push(t(&format!("doctor.{kind}"), default, &[("text", text)]));
    }
    lines.push(if problems == 0 {
        t("doctor.footer-ok", "&aNo problems found. Paste this output into a bug report if something still looks wrong.", &[])
    } else {
        t("doctor.footer-problems", "&c{count} problem(s) found. Fix them in config.yml, then &f/vdt reload&c.", &[("count", problems.to_string())])
    });
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::viewdistance_core::config::Config;

    fn engine() -> Engine {
        Engine::new(Config::parse(crate::viewdistance_core::DEFAULT_TOML).0)
    }

    fn obs() -> Observation {
        Observation { players: 2, mspt: 12.5, samples_ms: vec![12.5; 100], ..Default::default() }
    }

    const D: Distances = Distances { sim: 6, view: 12 };

    #[test]
    fn every_sub_command_is_checked_against_its_own_node() {
        assert_eq!(node_of("vdt"), None);
        assert_eq!(node_of("vdt foo"), None);
        assert_eq!(node_of("vdt reload"), Some("viewdistancetweaks.vdt.command.reload"));
        assert_eq!(node_of("vdt diagnose on"), Some("viewdistancetweaks.vdt.command.reload"));
        assert_eq!(node_of("vdt mspt debug"), Some("viewdistancetweaks.vdt.command.status"));
        assert_eq!(node_of("vdt history"), Some("viewdistancetweaks.vdt.command.status"));
        assert_eq!(node_of("vdt status --weight"), Some("viewdistancetweaks.vdt.command.status"));
        assert_eq!(node_of("VDT profile night-mode"), Some("viewdistancetweaks.vdt.command.profile"));
        assert_eq!(node_of("vdt viewdistance 10"), Some("viewdistancetweaks.vdt.command.viewdistance"));
        assert_eq!(node_of("vdt simulationdistance 8"), Some("viewdistancetweaks.vdt.command.simulationdistance"));
        assert_eq!(node_of("tpa Steve"), None);
        assert_eq!(node_of(""), None);
    }

    #[test]
    fn completion_follows_the_originals_lists() {
        let profiles: Vec<String> = ["normal", "event-mode"].iter().map(|s| s.to_string()).collect();
        assert_eq!(complete(&["s"], &profiles), ["status", "simulate", "simulationdistance"]);
        assert_eq!(complete(&["profile", "e"], &profiles), ["event-mode"]);
        assert_eq!(complete(&["profile", ""], &profiles), ["normal", "event-mode"]);
        assert_eq!(complete(&["viewdistance", "1"], &profiles), ["10", "12", "16"]);
        assert_eq!(complete(&["viewdistance", "8", ""], &profiles), ["world", "--duration"]);
        assert_eq!(complete(&["diagnose", "s"], &profiles), ["status"]);
        assert_eq!(complete(&["status", "--"], &profiles), ["--weight", "--fixed"]);
        assert!(complete(&["reload", ""], &profiles).is_empty());
    }

    #[test]
    fn other_commands_are_not_ours() {
        assert!(run(&mut engine(), "tpa Steve", D, &obs()).is_none());
        assert!(run(&mut engine(), "", D, &obs()).is_none());
        assert_eq!(run(&mut engine(), "vdt", D, &obs()).unwrap().lines, help());
    }

    #[test]
    fn reload_enable_disable() {
        let mut e = engine();
        assert!(run(&mut e, "vdt reload", D, &obs()).unwrap().reload);
        run(&mut e, "vdt disable", D, &obs());
        assert!(!e.active());
        run(&mut e, "VDT enable", D, &obs());
        assert!(e.active());
    }

    #[test]
    fn status_shows_mode_tick_time_and_chunk_counts() {
        let lines = run(&mut engine(), "vdt status", D, &obs()).unwrap().lines;
        assert_eq!(lines[1], "&7Mode: &fmixed &7| MSPT: &f12.50 &7| TPS: &f20.00 &7| Active: &ftrue");
        assert!(lines[2].contains("0.00ms"), "{lines:?}");
        // The startup grace is off by default, so the world line follows the variance line.
        // 2 players: (13^2) * 2 = 338 ticking, (25^2 - 13^2) * 2 = 912 not.
        assert!(lines[3].starts_with("&eworld") && lines[3].contains("Ticking: &f338 &7| Non-Ticking: &f912"), "{lines:?}");
        assert!(!lines[3].contains("Weight"), "{lines:?}");
        // The profile line is shown only while a profile other than "normal" is active.
        assert!(!lines.iter().any(|l| l.starts_with("&7Active profile")), "{lines:?}");
    }

    #[test]
    fn status_weight_shows_the_chunk_weight() {
        let lines = run(&mut engine(), "vdt status --weight", D, &obs()).unwrap().lines;
        assert!(lines.iter().any(|l| l.starts_with("&eworld") && l.contains("| Weight: &f1.0 &7| Next check")), "{lines:?}");
    }

    #[test]
    fn a_pin_with_a_duration_and_a_bad_value() {
        let mut e = engine();
        let r = run(&mut e, "vdt viewdistance 16 world --duration 5", D, &obs()).unwrap();
        assert_eq!(r.lines, vec!["&aView distance set for 5 min.".to_string()]);
        assert_eq!(e.tick(D, || unreachable!()).unwrap().to.view, 16);
        let r = run(&mut e, "vdt simulationdistance 40", D, &obs()).unwrap();
        assert!(r.lines[0].starts_with("&cInvalid value: &f40"));
        let r = run(&mut e, "vdt viewdistance 4", D, &obs()).unwrap();
        assert_eq!(r.lines.len(), 2);
        assert_eq!(r.lines[1], "&aView distance set (permanent until reload).");
        assert_eq!(
            run(&mut e, "vdt viewdistance x", D, &obs()).unwrap().lines,
            vec!["&cInvalid value: &fx&c. Must be between &f2&c and &f32&c.".to_string()]
        );
        assert_eq!(
            run(&mut e, "vdt viewdistance", D, &obs()).unwrap().lines,
            vec!["&cUsage: /vdt viewdistance <value> [world] [--duration <min>]".to_string()]
        );
        // A duration that is not a positive number means permanent, as in the original.
        let r = run(&mut e, "vdt viewdistance 8 --duration", D, &obs()).unwrap();
        assert_eq!(r.lines, vec!["&aView distance set (permanent until reload).".to_string()]);
        let r = run(&mut e, "vdt viewdistance 8 nether", D, &obs()).unwrap();
        assert_eq!(r.lines, vec!["&cWorld not found: nether".to_string()]);
    }

    #[test]
    fn simulation_distance_raises_the_view_like_the_original() {
        let mut e = engine();
        // D is sim 6 / view 12: a simulation distance of 14 needs the view raised to 14.
        let r = run(&mut e, "vdt simulationdistance 14", D, &obs()).unwrap();
        assert_eq!(r.lines.len(), 2);
        assert!(r.lines[0].contains("requested view distance 12 is below simulation distance 14"), "{:?}", r.lines);
        assert_eq!(e.tick(Distances { sim: 6, view: 12 }, || unreachable!()).unwrap().to.view, 14);
    }

    #[test]
    fn simulate_names_the_world_it_was_given() {
        let mut e = engine();
        assert_eq!(run(&mut e, "vdt simulate nether", D, &obs()).unwrap().lines, vec!["&cWorld not found: nether".to_string()]);
        assert!(run(&mut e, "vdt simulate", D, &obs()).unwrap().lines[0].ends_with("(all worlds) ==="));
    }

    #[test]
    fn profiles_are_listed_and_switched() {
        let mut e = engine();
        let lines = run(&mut e, "vdt profile", D, &obs()).unwrap().lines;
        assert_eq!(lines[1], "&7Available: &fnormal, event-mode, night-mode");
        assert_eq!(run(&mut e, "vdt profile event-mode", D, &obs()).unwrap().lines[0], "&aSwitched to profile &fevent-mode&a.");
        assert_eq!(e.config().cap_view, Some(8));
        assert!(run(&mut e, "vdt profile rave", D, &obs()).unwrap().lines[0].starts_with("&cUnknown profile: rave"));
    }

    #[test]
    fn history_lists_newest_first() {
        let mut e = Engine::new(Config::parse("[startup-grace]\nenabled = true\nsimulation-distance = 5\nview-distance = 10\n").0);
        assert!(run(&mut e, "vdt history", D, &obs()).unwrap().lines[0].starts_with("&7No distance changes"));
        e.tick(D, || unreachable!()); // the startup grace: 6/12 -> 5/10
        let lines = run(&mut e, "vdt history", D, &obs()).unwrap().lines;
        assert_eq!(lines[0], "&6=== Last 1 distance changes ===");
        assert_eq!(lines[1], "&80s ago &eworld &7Sim: &f6&7\u{2192}&a5 &7View: &f12&7\u{2192}&a10");
        assert_eq!(lines[2], "  &8startup grace");
    }

    #[test]
    fn mspt_and_simulate_and_diagnose() {
        let mut e = engine();
        let lines = run(&mut e, "vdt mspt", D, &obs()).unwrap().lines;
        assert_eq!(lines[0], "&7MSPT (avg): &f12.50");
        assert_eq!(lines.len(), 3, "the sample lines are for mspt debug only: {lines:?}");
        let lines = run(&mut e, "vdt mspt debug", D, &obs()).unwrap().lines;
        assert_eq!(lines[3], "&7Samples: &f100 &7(source: the server's tick times)");
        assert_eq!(lines[5], "&7Last 10: &f12.5 12.5 12.5 12.5 12.5 12.5 12.5 12.5 12.5 12.5");
        let lines = run(&mut e, "vdt simulate", D, &obs()).unwrap().lines;
        assert_eq!(lines.len(), 5);
        let r = run(&mut e, "vdt diagnose note lag at spawn", D, &obs()).unwrap();
        assert_eq!(r.diagnose, Some(Diagnose::Note("lag at spawn".into())));
        assert_eq!(run(&mut e, "vdt diagnose", D, &obs()).unwrap().diagnose, Some(Diagnose::Status));
        assert_eq!(run(&mut e, "vdt diagnose status", D, &obs()).unwrap().diagnose, Some(Diagnose::Status));
        assert_eq!(
            run(&mut e, "vdt diagnose note", D, &obs()).unwrap().lines,
            vec!["&cUsage: /vdt diagnose note <text>".to_string()]
        );
        assert_eq!(run(&mut e, "vdt diagnose on", D, &obs()).unwrap().diagnose, Some(Diagnose::On));
    }

    #[test]
    fn presets_analyze_and_doctor() {
        let mut e = engine();
        let r = run(&mut e, "vdt preset large-network", D, &obs()).unwrap();
        assert_eq!(r.lines, vec!["&aApplied preset &flarge-network&a (9 values changed). Config reloaded - fine-tune further as needed.".to_string()]);
        assert_eq!((e.config().max_sim, e.config().max_view, e.config().increase_mspt), (8, 12, 38.0));
        let r = run(&mut e, "vdt preset nope", D, &obs()).unwrap();
        assert_eq!(r.lines, vec!["&cUnknown preset: nope &7(available: small-server, large-network, performance-first)".to_string()]);
        assert_eq!(run(&mut e, "vdt preset", D, &obs()).unwrap().lines, vec!["&cUsage: /vdt preset <small-server|large-network|performance-first>".to_string()]);
        // Twelve ms flat against the default thresholds: nothing to change but the ceiling.
        let a = run(&mut e, "vdt analyze", D, &obs()).unwrap().lines;
        assert_eq!(a[0], "&6--- Config analysis --- &7(suggestions only, nothing is changed)");
        assert!(a[1].starts_with("&e~ &fworld-settings.<world>.max-view-distance"), "{}", a[1]);
        assert!(a.last().unwrap().contains("/vdt reload"));
        let few = Observation { samples_ms: vec![1.0; 3], ..obs() };
        assert_eq!(run(&mut e, "vdt analyse", D, &few).unwrap().lines[1], "&a+ &fNot enough data yet - leave it running for an hour or so.");
        let d = run(&mut e, "vdt doctor", D, &obs()).unwrap().lines;
        assert!(d[1].starts_with("&a+ &fMSPT thresholds"), "{}", d[1]);
        assert!(d.last().unwrap().starts_with("&aNo problems found."));
    }
}
