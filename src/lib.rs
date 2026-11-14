//! ViewDistance as an arismoko native mod (trait mod on `mod_api`). The decision logic is the
//! unchanged `viewdistance-core` crate; this file only wires it to hooks, host methods and commands.
//!
//! The world's view and simulation distance are set live (`server_set_view_distance`, ...). What
//! arismoko does not have yet is marked `TODO(arismoko-api: <name>)` and listed in PORT_NOTES.md:
//! per-player distances, idle time, the generated-chunk total. Until then the throttles see 0 idle
//! ticks and the per-player changes are logged only.
//!
//! Config: `config/viewdistance.toml`, registered with `register_configs` so the game writes the
//! defaults when it is missing. The distances the server starts with come from `server.properties`.
use mod_api::{
    Command, CommandArgumentValue, Config, ConfigKind, ConfigLoadingEvent, ConfigReloadingEvent,
    Flow, HostSide, Hooks, LevelRef, PlayerRef, RegisterCommandsEvent, RegisterConfigsEvent,
    PlayerCommandEvent, RunModCommandEvent, ServerAboutToStartEvent, ServerHost, ServerStoppingEvent, ServerTickEvent,
    ServerTickPhase, Text,
};
use std::sync::Mutex;
use viewdistance_core::command;
use viewdistance_core::{Change, Distances, Event, Observation, PlayerSample};

const ID: &str = "viewdistance";
const CONFIG_FILE: &str = "viewdistance.toml";
const OVERWORLD: &str = "minecraft:overworld";
/// The level the original asks for its fixed-distance nodes: a permissions mod must grant them,
/// the operator fallback stays out (no arismoko operator level reaches 5).
const FIXED_LEVEL: u8 = 5;

#[derive(Default)]
struct Mod {
    /// The view and simulation distance the server started with, from `server.properties`.
    actual: Mutex<Option<Distances>>,
}

impl Mod {
    fn actual(&self) -> Distances {
        self.actual.lock().unwrap_or_else(|e| e.into_inner()).unwrap_or(Distances { sim: 10, view: 10 })
    }

    /// What the server runs at right now (`server_view_distance` / `server_simulation_distance`), the
    /// start values where the host gives none. The engine compares its pins and decisions with this:
    /// feeding it the start values made every tick see the old distance and re-apply the change.
    fn live(&self, host: &dyn ServerHost) -> Distances {
        let start = self.actual();
        Distances {
            sim: host.server_simulation_distance().unwrap_or(start.sim),
            view: host.server_view_distance().unwrap_or(start.view),
        }
    }
}

fn log(line: &str) {
    eprintln!("[viewdistance] {line}");
}

fn load(text: &str) {
    for problem in viewdistance_core::load_config(text) {
        log(&format!("config: {problem}"));
    }
}

/// `server.properties` `view-distance` and `simulation-distance` (vanilla's 10 when missing).
fn properties_distances() -> Distances {
    let text = std::fs::read_to_string("server.properties").unwrap_or_default();
    let get = |key: &str| {
        text.lines()
            .find_map(|l| l.strip_prefix(key)?.strip_prefix('=')?.trim().parse::<i32>().ok())
            .unwrap_or(10)
    };
    Distances { view: get("view-distance"), sim: get("simulation-distance") }
}

/// The line with its `&` colour codes as Text (`text_from_legacy`); the plain text when that refuses it.
fn coloured(host: &dyn ServerHost, line: &str) -> Text {
    host.text_from_legacy(line)
        .unwrap_or_else(|_| Text::literal(plain(line)))
}

/// The text without its `&` colour codes.
fn plain(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        let code = c == '&' && chars.peek().is_some_and(|k| k.is_ascii_hexdigit() || matches!(k.to_ascii_lowercase(), 'k'..='o' | 'r'));
        if code {
            chars.next();
        } else {
            out.push(c);
        }
    }
    out
}

/// Whether `player` holds `node`: the value a permission provider gives, or the operator fallback
/// `level` when none decided.
fn grants(host: &dyn ServerHost, player: PlayerRef, node: &str, level: u8) -> bool {
    player
        .permission_value(host, node)
        .unwrap_or_else(|| player.permission_level(host).unwrap_or(0) >= level)
}

fn overworld(host: &dyn ServerHost) -> Option<LevelRef> {
    host.levels().list_levels().into_iter().find(|l| l.dimension_id() == OVERWORLD)
}

fn argument_text(value: &CommandArgumentValue) -> Option<String> {
    match value {
        CommandArgumentValue::String(s) => Some(s.clone()),
        CommandArgumentValue::Integer(i) => Some(i.to_string()),
        CommandArgumentValue::Number(n) => Some(n.to_string()),
        CommandArgumentValue::Boolean(b) => Some(b.to_string()),
        _ => None,
    }
}

/// `<name:kind>` (required) or `[name:kind]` (optional): the argument's name and whether it is optional.
fn placeholder(token: &str) -> Option<(&str, bool)> {
    let (inner, optional) = match token.strip_prefix('<').and_then(|t| t.strip_suffix('>')) {
        Some(inner) => (inner, false),
        None => (token.strip_prefix('[')?.strip_suffix(']')?, true),
    };
    Some((inner.split(':').next().unwrap_or(inner), optional))
}

/// The `/vdt` line as it was typed. The host hands over the words and the arguments apart, so the
/// usage with the same words and argument names is found in `USAGES` and its placeholders are
/// filled back in order. `literals` starts with the command name, `present` holds each argument
/// that was given, by name.
fn typed_line(literals: &[String], present: &[(String, String)]) -> Option<String> {
    'usage: for usage in command::USAGES {
        let tokens: Vec<&str> = usage.split_whitespace().collect();
        let words: Vec<&str> = tokens.iter().copied().filter(|t| placeholder(t).is_none()).collect();
        if words.iter().copied().ne(literals.iter().map(String::as_str)) {
            continue;
        }
        let names: Vec<&str> = tokens.iter().filter_map(|t| placeholder(t).map(|(n, _)| n)).collect();
        if present.iter().any(|(name, _)| !names.contains(&name.as_str())) {
            continue;
        }
        let mut parts = Vec::new();
        for token in tokens {
            match placeholder(token) {
                None => parts.push(token.to_string()),
                Some((name, optional)) => match present.iter().find(|(n, _)| n == name) {
                    Some((_, value)) => parts.push(value.clone()),
                    None if optional => {}
                    None => continue 'usage,
                },
            }
        }
        return Some(parts.join(" "));
    }
    None
}

/// The tick time (ms, average of the last 100 ticks) and the ticks a second the server keeps up.
/// `tick_stats` gives the rate and the average; the per-tick samples are not available.
fn tick_figures(host: &dyn ServerHost) -> (f64, f64) {
    let (rate, mspt) = host.tick_stats();
    let rate = f64::from(rate);
    let tps = if mspt > 0.0 { rate.min(1000.0 / mspt) } else { rate };
    (mspt, tps.min(20.0))
}

/// The last 100 tick durations in ms, the ones recorded so far (`tick_times` is in ns, 0 where none).
fn tick_samples_ms(host: &dyn ServerHost) -> Vec<f64> {
    host.tick_times()
        .into_iter()
        .filter(|&t| t > 0)
        .map(|t| t as f64 / 1_000_000.0)
        .collect()
}

/// What a check or a `/vdt` answer looks at, from the running server.
/// TODO(arismoko-api: level_chunks_generated): `generated_total` stays 0.
fn observation(host: &dyn ServerHost, players: &[PlayerRef], loaded: u32, with_chunks: bool) -> Observation {
    let (mspt, tps) = tick_figures(host);
    let player_chunks = if with_chunks { player_chunks(host, players) } else { Vec::new() };
    Observation {
        players: players.len() as u32,
        mspt,
        samples_ms: tick_samples_ms(host),
        loaded_chunks: loaded,
        generated_total: 0,
        free_memory_percent: viewdistance_core::free_memory_percent(),
        tps: Some(tps),
        player_chunks,
    }
}

/// Each player's dimension (a small number that tells them apart) and chunk, for density.
fn player_chunks(host: &dyn ServerHost, players: &[PlayerRef]) -> Vec<(u8, i32, i32)> {
    players
        .iter()
        .filter_map(|p| {
            let level = p.entity().level(host)?;
            let at = p.entity().position(host)?;
            let dim = level.dimension_id().bytes().fold(0u8, |h, b| h.wrapping_mul(31).wrapping_add(b));
            Some((dim, (at[0].floor() as i32) >> 4, (at[2].floor() as i32) >> 4))
        })
        .collect()
}

fn loaded_chunks(host: &dyn ServerHost) -> u32 {
    // The overworld's loaded chunks: the length of `loaded_chunks`, as the original's level_stats count.
    overworld(host).and_then(|l| l.loaded_chunks(host)).map_or(0, |c| c.len() as u32)
}

/// Sets the server's view and simulation distance, live (not written to `server.properties`).
/// Returns whether either setter took the value.
fn apply_world_distances(host: &mut dyn ServerHost, change: &Change) -> bool {
    let view = host.server_set_view_distance(change.to.view);
    let sim = host.server_set_simulation_distance(change.to.sim);
    view || sim
}

/// The per-player view and simulation distances from the personal throttles (AFK, ping, fixed by
/// permission). TODO(arismoko-api: player_set_view_distance) and
/// TODO(arismoko-api: player_set_simulation_distance): nothing is sent, so these are dropped.
fn apply_personal(_host: &mut dyn ServerHost, _targets: &[(u128, Option<Distances>)]) {}

/// One server tick: the proactive and reactive checks, the personal throttles, relief, notices and
/// diagnostics, as the original does at the end of a tick.
fn check(host: &mut dyn ServerHost, base: Distances) {
    let players = host.players().server_players();
    let loaded = loaded_chunks(&*host);
    let change = {
        let view: &dyn ServerHost = &*host;
        viewdistance_core::tick(base, || observation(view, &players, loaded, true))
    };
    if let Some(c) = change.as_ref() {
        let applied = apply_world_distances(host, c);
        if viewdistance_core::log_changes() {
            let note = if applied { "" } else { "; not applied" };
            log(&format!(
                "Sim {} -> {} | View {} -> {} ({}){note}",
                c.from.sim, c.to.sim, c.from.view, c.to.view, c.reason
            ));
        }
    }

    // Personal distances: AFK and ping throttles, and `viewdistancetweaks.fixed.<n>` /
    // `.simfixed.<n>` by permission. Idle time is not available (TODO(arismoko-api:
    // player_idle_ticks)), so the AFK throttle sees 0.
    let (view_prefix, sim_prefix) = viewdistance_core::fixed_prefixes();
    let samples: Vec<PlayerSample> = if viewdistance_core::personal_due() {
        players
            .iter()
            .map(|p| {
                let view: &dyn ServerHost = &*host;
                PlayerSample {
                    id: p.uuid(),
                    idle_ticks: 0,
                    ping_ms: p.latency(view).unwrap_or(0),
                    fixed_view: view_prefix.as_deref().and_then(|prefix| {
                        viewdistance_core::fixed_distance(prefix, |node| grants(view, *p, node, FIXED_LEVEL))
                    }),
                    fixed_sim: sim_prefix.as_deref().and_then(|prefix| {
                        viewdistance_core::fixed_distance(prefix, |node| grants(view, *p, node, FIXED_LEVEL))
                    }),
                }
            })
            .collect()
    } else {
        Vec::new()
    };
    let mut personal = viewdistance_core::personal(|| samples);
    if change.as_ref().is_some_and(|c| c.to.view != c.from.view) {
        // The world's distance was set again, so everyone is back on it.
        personal.extend(viewdistance_core::throttled().into_iter().map(|(id, d, _)| (id, Some(d))));
    }
    apply_personal(host, &personal);

    // Notices go to the players who hold `viewdistancetweaks.notify` (operators of `notify-min-permission`).
    let events = viewdistance_core::take_events();
    if !events.is_empty() {
        let notify_level = viewdistance_core::permissions().0.max(1);
        let notified: Vec<PlayerRef> = players
            .iter()
            .copied()
            .filter(|p| grants(&*host, *p, command::NOTIFY_NODE, notify_level))
            .collect();
        for event in events {
            match event {
                Event::Announce(line) => {
                    for p in &notified {
                        let text = coloured(&*host, &line);
                        let _ = p.tell(host, text);
                    }
                }
                Event::ReliefStart => {
                    let (rain, thunder, advance) = match overworld(&*host) {
                        Some(level) => {
                            let (rain, thunder) = level.weather(&*host).unwrap_or((0.0, 0.0));
                            let advance = matches!(level.game_rule(&*host, "advance_time"), Ok(Some(v)) if v.to_string() == "true");
                            (rain, thunder, advance)
                        }
                        None => (0.0, 0.0, true),
                    };
                    for command in viewdistance_core::relief_start(rain > 0.2, thunder > 0.9, advance) {
                        let _ = host.commands().run_command(&command);
                    }
                    log("WARN emergency relief on");
                }
                Event::ReliefEnd => {
                    for command in viewdistance_core::relief_end() {
                        let _ = host.commands().run_command(&command);
                    }
                    log("emergency relief over");
                }
            }
        }
    }

    if let Some(world) = host.world_dir() {
        let dir = world.join("viewdistance").join("diagnostics");
        let unix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let (mspt, _) = tick_figures(&*host);
        let (players_now, loaded) = (players.len() as u32, loaded_chunks(&*host));
        viewdistance_core::diagnostics(&dir, unix, change, mspt, || viewdistance_core::diag::Sample {
            mspt,
            players: players_now,
            loaded_chunks: loaded,
            distances: base,
            free_memory_percent: viewdistance_core::free_memory_percent(),
            relief: viewdistance_core::relief_active(),
            profile: viewdistance_core::profile_name(),
        });
    }
}

#[mod_api::horizon_mod]
impl Hooks for Mod {
    fn register_configs(&self, _: HostSide, event: &mut RegisterConfigsEvent<'_>) {
        let mut config = Config::new(ID, ConfigKind::Common, viewdistance_core::DEFAULT_TOML);
        config.file = CONFIG_FILE.to_string();
        event.configs.push(config);
    }

    fn config_loading(&self, _: HostSide, event: &mut ConfigLoadingEvent<'_>) {
        if event.config.mod_id == ID {
            load(&event.config.text);
        }
    }

    fn config_reloading(&self, _: HostSide, event: &mut ConfigReloadingEvent<'_>) {
        if event.config.mod_id == ID {
            load(&event.config.text);
            log("config/viewdistance.toml read again");
        }
    }

    fn server_about_to_start(&self, _: &mut dyn ServerHost, _: &mut ServerAboutToStartEvent) {
        let base = properties_distances();
        *self.actual.lock().unwrap_or_else(|e| e.into_inner()) = Some(base);
        log(&format!("server starts at view {} sim {}", base.view, base.sim));
    }

    fn register_commands(&self, _: &mut dyn ServerHost, event: &mut RegisterCommandsEvent<'_>) {
        // Bukkit gates all of /vdt with the one plugin.yml `permission:` `viewdistancetweaks.command.vdt`
        // (default op); the sub-nodes are only declared there. So every usage is gated by the root node
        // alone: a holder of it runs every sub-command, a holder of only a sub-node runs nothing.
        // The operator level decides when no permissions mod answers.
        let level = viewdistance_core::permissions().1;
        for usage in command::USAGES.iter() {
            event.commands.push(Command::op(usage, level).with_node("viewdistancetweaks.command.vdt"));
        }
    }

    fn player_command(&self, host: &mut dyn ServerHost, event: &mut PlayerCommandEvent<'_>) -> Flow {
        // plugin.yml gives `/vdt` the node `viewdistancetweaks.command.vdt` (default op): a player without it
        // gets Bukkit's no-permission line, not "Unknown command". The node is the provider's answer, else the
        // operator level `command-min-permission`.
        let root = event.command.split_whitespace().next().unwrap_or("").to_ascii_lowercase();
        if root != "vdt" {
            return Flow::Continue;
        }
        let level = viewdistance_core::permissions().1;
        let allowed = host.player(event.player).is_some_and(|p| grants(&*host, p, "viewdistancetweaks.command.vdt", level));
        if allowed {
            return Flow::Continue;
        }
        event.replies.push(Text::literal("\u{a7}cI'm sorry, but you do not have permission to perform this command. Please contact the server administrators if you believe that this is in error."));
        event.cancelled = true;
        Flow::Cancel
    }

    fn run_mod_command(&self, host: &mut dyn ServerHost, event: &mut RunModCommandEvent<'_>) -> Flow {
        if event.command.literals.first().map(String::as_str) != Some("vdt") {
            return Flow::Continue;
        }
        let present: Vec<(String, String)> = event
            .command
            .arguments
            .iter()
            .filter_map(|a| argument_text(&a.value).map(|t| (a.name.clone(), t)))
            .collect();
        let Some(line) = typed_line(&event.command.literals, &present) else {
            return Flow::Continue;
        };
        let base = self.live(&*host);
        let players = host.players().server_players();
        let loaded = loaded_chunks(&*host);
        let obs = observation(&*host, &players, loaded, false);
        let Some(reply) = viewdistance_core::command(&line, base, &obs) else {
            return Flow::Continue;
        };
        let mut lines = reply.lines;
        if reply.reload {
            let path = host.instance_dir().join("config").join(CONFIG_FILE);
            match std::fs::read_to_string(&path) {
                Ok(text) => {
                    lines.push("&aConfiguration reloaded.".into());
                    lines.extend(viewdistance_core::load_config(&text).into_iter().map(|p| format!("&e{p}")));
                }
                Err(e) => lines.push(format!("&c{e}")),
            }
        }
        if let (Some(d), Some(world)) = (reply.diagnose.as_ref(), host.world_dir()) {
            let unix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
            viewdistance_core::diagnose(d, &world.join("viewdistance").join("diagnostics"), unix);
        }
        for line in lines {
            let text = coloured(&*host, &line);
            let _ = host.commands().feedback(text);
        }
        Flow::Cancel
    }

    fn server_tick(&self, host: &mut dyn ServerHost, event: &mut ServerTickEvent<'_>) {
        if !matches!(event.phase, ServerTickPhase::ServerTickEnd { world_running: true }) {
            return;
        }
        let base = self.live(&*host);
        check(host, base);
    }

    fn server_stopping(&self, host: &mut dyn ServerHost, _: &mut ServerStoppingEvent) {
        // Emergency relief is undone before the world is saved, since the game rules are saved with it.
        for command in viewdistance_core::relief_end() {
            let _ = host.commands().run_command(&command);
        }
    }
}
