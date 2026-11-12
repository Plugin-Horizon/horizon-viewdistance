//! `/vdt`: the answers, as `&`-coded lines. The texts are the original's English defaults; the
//! server's one world is called "world".
use crate::engine::{std_dev, Distances, Engine, Observation};

/// The usages registered with the server (completion and operator level): one per real sub-command
/// path, with literals for the fixed words and typed arguments for the rest. `--duration` and the
/// `--fixed`/`--weight` flags are literal words (the flags in either order); a usage with a required
/// argument after an optional one is refused by the server, so the optional world comes before it
/// only where it is the last argument.
pub const USAGES: [&str; 27] = [
    "vdt",
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
    "vdt viewdistance <value:integer>",
    "vdt viewdistance <value:integer> <world:word>",
    "vdt viewdistance <value:integer> --duration <minutes:integer>",
    "vdt viewdistance <value:integer> <world:word> --duration <minutes:integer>",
    "vdt simulationdistance <value:integer>",
    "vdt simulationdistance <value:integer> <world:word>",
    "vdt simulationdistance <value:integer> --duration <minutes:integer>",
    "vdt simulationdistance <value:integer> <world:word> --duration <minutes:integer>",
    "vdt diagnose",
    "vdt diagnose on",
    "vdt diagnose off",
    "vdt diagnose note <text:text>",
];

/// The permission node of each usage above, in the same order (the real plugin's nodes: one per
/// sub-command group, `viewdistancetweaks.command.vdt` for the bare command and help). A server
/// with a permissions mod can grant them; without one the operator level decides. Note: the host
/// takes the node of the first usage with one for the whole `vdt` command (see GAPS.md).
pub const NODES: [&str; 27] = [
    "viewdistancetweaks.command.vdt",
    "viewdistancetweaks.command.vdt",
    "viewdistancetweaks.vdt.command.reload",
    "viewdistancetweaks.vdt.command.reload",
    "viewdistancetweaks.vdt.command.reload",
    "viewdistancetweaks.vdt.command.status",
    "viewdistancetweaks.vdt.command.status",
    "viewdistancetweaks.vdt.command.status",
    "viewdistancetweaks.vdt.command.status",
    "viewdistancetweaks.vdt.command.status",
    "viewdistancetweaks.vdt.command.status",
    "viewdistancetweaks.vdt.command.simulate",
    "viewdistancetweaks.vdt.command.status",
    "viewdistancetweaks.vdt.command.status",
    "viewdistancetweaks.vdt.command.profile",
    "viewdistancetweaks.vdt.command.viewdistance",
    "viewdistancetweaks.vdt.command.viewdistance",
    "viewdistancetweaks.vdt.command.viewdistance",
    "viewdistancetweaks.vdt.command.viewdistance",
    "viewdistancetweaks.vdt.command.simulationdistance",
    "viewdistancetweaks.vdt.command.simulationdistance",
    "viewdistancetweaks.vdt.command.simulationdistance",
    "viewdistancetweaks.vdt.command.simulationdistance",
    "viewdistancetweaks.vdt.command.reload",
    "viewdistancetweaks.vdt.command.reload",
    "viewdistancetweaks.vdt.command.reload",
    "viewdistancetweaks.vdt.command.reload",
];

/// The permission node a `/vdt` line needs (without the slash; the node of the longest usage whose
/// literal words it starts with); None when the line is not `/vdt`.
pub fn node_of(line: &str) -> Option<&'static str> {
    let words: Vec<&str> = line.split_whitespace().collect();
    if !words.first()?.eq_ignore_ascii_case("vdt") {
        return None;
    }
    let mut best: Option<(usize, &'static str)> = None;
    for (usage, node) in USAGES.iter().zip(NODES) {
        let tokens: Vec<&str> = usage.split_whitespace().collect();
        let literal = tokens.iter().take_while(|t| !t.starts_with(['<', '['])).count();
        let matches = literal <= words.len() && tokens[..literal].iter().zip(&words).all(|(a, b)| a.eq_ignore_ascii_case(b));
        if matches && best.is_none_or(|(l, _)| literal > l) {
            best = Some((literal, node));
        }
    }
    best.map(|(_, node)| node)
}

/// The permission node of the in-chat notices of distance changes.
pub const NOTIFY_NODE: &str = "viewdistancetweaks.notify";

/// Where diagnostics are written, inside the world's folder (shown to operators).
pub const DIAG_FOLDER: &str = "viewdistance/diagnostics";

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
    Note(String),
}

fn reply(lines: Vec<String>) -> Option<Reply> {
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
/// analyze, doctor, preset, pregen and update are missing (see GAPS.md).
pub fn help() -> Vec<String> {
    [
        "&6--- VDT Commands ---",
        "&e/vdt reload                              &7- Reload configuration",
        "&e/vdt status [--weight|--fixed]           &7- Show status",
        "&e/vdt simulate [world]                    &7- Preview next check without applying it",
        "&e/vdt history                             &7- Show recent distance changes",
        "&e/vdt diagnose [on|off|note <text>] &7- Record what the plugin sees to a log file",
        "&e/vdt simulationdistance <value> [world] [--duration <min>]",
        "&e/vdt viewdistance <value> [world] [--duration <min>]",
        "&e/vdt enable / disable                    &7- Toggle the plugin",
        "&e/vdt mspt                                &7- Show MSPT, TPS & variance",
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
            reply(vec!["&aPlugin enabled.".into()])
        }
        Some("disable") => {
            e.switch(false);
            reply(vec!["&cPlugin disabled.".into()])
        }
        Some("status") => reply(status(e, actual, o, rest.contains(&"--fixed"), rest.contains(&"--weight"))),
        Some("mspt") => reply(mspt(e, o, rest.first() == Some(&"debug"))),
        Some("history") => reply(history(e)),
        Some("simulate") => reply(simulate(e, actual, o, rest.first().copied())),
        Some("profile") => reply(profile(e, rest.first().copied())),
        Some(which @ ("viewdistance" | "simulationdistance")) => reply(pin(e, which == "simulationdistance", &rest, actual)),
        Some("diagnose") => {
            let d = match rest.first().map(|w| w.to_ascii_lowercase()).as_deref() {
                Some("on") => Diagnose::On,
                Some("off") => Diagnose::Off,
                Some("note") if rest.len() > 1 => Diagnose::Note(rest[1..].join(" ")),
                Some("note") => return reply(vec!["&cUsage: /vdt diagnose note <text>".into()]),
                _ => return reply(vec!["&cUsage: /vdt diagnose [on|off|note <text>]".into()]),
            };
            let line = match &d {
                Diagnose::On => "&aDiagnostics are recording to &f{folder}&a, one file per day. &7/vdt diagnose off stops it.",
                Diagnose::Off => "&7Diagnostics are off. &f/vdt diagnose on &7starts recording to &f{folder}&7.",
                Diagnose::Note(_) => "&aNote saved.",
            }
            .replace("{folder}", DIAG_FOLDER);
            Some(Reply { lines: vec![line.into()], diagnose: Some(d), ..Reply::default() })
        }
        _ => reply(help()),
    }
}

fn status(e: &Engine, d: Distances, o: &Observation, fixed: bool, weight: bool) -> Vec<String> {
    let c = e.config();
    if fixed {
        let throttled = e.throttled();
        if !c.afk && !c.ping && !c.fixed_view && !c.fixed_sim {
            return vec![
                "&7No per-player distance feature is enabled in config.yml &8(per-player-fixed-view-distance, \
                 per-player-fixed-simulation-distance, afk-player-throttling, ping-throttling)."
                    .into(),
            ];
        }
        let mut lines = vec!["&6=== Active per-player overrides ===".to_string()];
        if throttled.is_empty() {
            lines.push("&7No player currently has a fixed view or simulation distance.".into());
        }
        for (id, p, by) in throttled {
            let source = match by {
                "PING" => "&b[PING]",
                "FIXED" => "&d[FIXED]",
                _ => "&6[AFK]",
            };
            lines.push(format!("&e{id:032x} &7in &fworld"));
            lines.push(format!("  &7View: &a{} &8(world: {}) {source}", p.view, d.view));
            lines.push(format!("  &7Sim:  &a{} &8(world: {}) {source}", p.sim, d.sim));
        }
        return lines;
    }
    let mode = format!("{:?}", c.mode).to_ascii_lowercase();
    let mut lines = vec![
        "&6=== ViewDistanceTweaks Status ===".to_string(),
        format!("&7Mode: &f{mode} &7| MSPT: &f{} &7| TPS: &f{} &7| Active: &f{}", f2(o.mspt), f2(o.tps.unwrap_or_else(|| tps(o.mspt))), e.active()),
    ];
    if o.samples_ms.len() >= c.variance_min_samples {
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
        lines.push("&7MSPT variance: &8not enough samples yet".into());
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
        lines.push(format!("&7Active profile: &b{}", e.profile()));
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
        format!("&7MSPT stddev: &f{} &7(threshold: &f{}&7)", f2(std_dev(s)), f2(e.config().variance_threshold)),
    ];
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
        return vec!["&7No distance changes recorded yet since the last restart.".into()];
    }
    let mut lines = vec![format!("&6=== Last {} distance changes ===", entries.len())];
    for (tick, c) in entries.into_iter().rev() {
        lines.push(format!(
            "&8{} &eworld &7Sim: &f{}&7\u{2192}&a{} &7View: &f{}&7\u{2192}&a{}",
            ago(e.ticks() - tick),
            c.from.sim,
            c.to.sim,
            c.from.view,
            c.to.view
        ));
        lines.push(format!("  &8{}", c.reason));
    }
    lines
}

fn simulate(e: &Engine, cur: Distances, o: &Observation, world: Option<&str>) -> Vec<String> {
    // The original checks "enabled" before the world name, so a disabled plugin answers first.
    if !e.active() {
        return vec!["&cPlugin is disabled - enable it first (/vdt enable) to simulate a check.".into()];
    }
    // The server's one world is called "world" (see the module comment).
    if let Some(w) = world.filter(|w| *w != "world") {
        return vec![format!("&cWorld not found: {w}")];
    }
    let (to, reason, now) = e.simulate(cur, o);
    // Original colouring (simulate.entry): the arrow is green when the next check would apply and
    // grey when not; the new values are yellow when anything changes and grey when nothing does.
    let arrow = if now { "&a\u{2192}" } else { "&8\u{2192}" };
    let colour = if to == cur { "&7" } else { "&e" };
    vec![
        format!("&6=== /vdt simulate {} ===", world.unwrap_or("(all worlds)")),
        "&7This shows what the NEXT check would compute. Nothing is applied.".into(),
        format!("&7Current MSPT: &f{} &7| stddev: &f{}", f2(o.mspt), f2(std_dev(&o.samples_ms))),
        format!(
            "&eworld &7| Sim: &f{} {arrow} {colour}{} &7| View: &f{} {arrow} {colour}{}",
            cur.sim, to.sim, cur.view, to.view
        ),
        format!(
            "  &8{reason}{}",
            if now && to != cur { " &a[would apply]" } else { " &8[not yet / no change]" }
        ),
    ]
}

fn profile(e: &mut Engine, name: Option<&str>) -> Vec<String> {
    let names = e.base_config().profile_names().join(", ");
    match name {
        None => vec![format!("&7Active profile: &b{}", e.profile()), format!("&7Available: &f{names}")],
        Some(n) if e.set_profile(n) => vec![format!("&aSwitched to profile &f{}&a.", e.profile())],
        Some(n) => vec![format!("&cUnknown profile: {n} &7(available: {names})")],
    }
}

/// `/vdt viewdistance|simulationdistance <value> [world] [--duration <min>]`.
fn pin(e: &mut Engine, sim: bool, args: &[&str], cur: Distances) -> Vec<String> {
    let usage = if sim {
        "&cUsage: /vdt simulationdistance <value> [world] [--duration <min>]"
    } else {
        "&cUsage: /vdt viewdistance <value> [world] [--duration <min>]"
    };
    let Some(Ok(value)) = args.first().map(|v| v.parse::<i32>()) else {
        return vec![usage.into()];
    };
    if !(2..=32).contains(&value) {
        return vec![format!("&cInvalid value: &f{value}&c. Must be between &f2&c and &f32&c.")];
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
        return vec![format!("&cWorld not found: {w}")];
    }
    let mut lines = Vec::new();
    let mut value = value;
    if !sim && value < cur.sim {
        lines.push(format!(
            "&eworld: requested view distance {value} is below simulation distance {} - using {} instead (view must be >= sim).",
            cur.sim, cur.sim
        ));
        value = cur.sim;
    }
    // Simulation distance: the view is raised to it, with the same message and the same duration,
    // as the original does when it applies the pair.
    let mut view_pin = None;
    if sim && cur.view < value {
        lines.push(format!(
            "&eworld: requested view distance {} is below simulation distance {value} - using {value} instead (view must be >= sim).",
            cur.view
        ));
        view_pin = Some(value);
    }
    e.pin(sim, value, minutes);
    if let Some(view) = view_pin {
        e.pin(false, view, minutes);
    }
    let label = if sim { "Simulation distance" } else { "View distance" };
    lines.push(match minutes {
        Some(m) => format!("&a{label} set for {m} min."),
        None => format!("&a{label} set (permanent until reload)."),
    });
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn engine() -> Engine {
        Engine::new(Config::parse(crate::DEFAULT_TOML).0)
    }

    fn obs() -> Observation {
        Observation { players: 2, mspt: 12.5, samples_ms: vec![12.5; 100], ..Default::default() }
    }

    const D: Distances = Distances { sim: 6, view: 12 };

    #[test]
    fn every_sub_command_is_checked_against_its_own_node() {
        assert_eq!(node_of("vdt"), Some("viewdistancetweaks.command.vdt"));
        assert_eq!(node_of("vdt reload"), Some("viewdistancetweaks.vdt.command.reload"));
        assert_eq!(node_of("vdt status --weight"), Some("viewdistancetweaks.vdt.command.status"));
        assert_eq!(node_of("VDT profile night-mode"), Some("viewdistancetweaks.vdt.command.profile"));
        assert_eq!(node_of("vdt viewdistance 10"), Some("viewdistancetweaks.vdt.command.viewdistance"));
        assert_eq!(node_of("vdt simulationdistance reset"), Some("viewdistancetweaks.vdt.command.simulationdistance"));
        assert_eq!(node_of("tpa Steve"), None);
        assert_eq!(node_of(""), None);
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
        assert!(run(&mut e, "vdt viewdistance x", D, &obs()).unwrap().lines[0].starts_with("&cUsage"));
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
        assert_eq!(run(&mut e, "vdt diagnose on", D, &obs()).unwrap().diagnose, Some(Diagnose::On));
    }
}
