//! ViewDistance as an arismoko native mod (trait mod on `mod_api`). The decision logic is the
//! unchanged `viewdistance-core` crate; this file only wires it to hooks, host methods and commands.
//!
//! The world's view and simulation distance are set live (`server_set_view_distance`, ...). What
//! Horizon main does not have yet is marked `HMOD-GAP(<name>)` and listed in HMOD_PORT.md: per-player
//! distances, idle time, player ping. Until then the throttles see 0 idle ticks and 0 ms.
//!
//! Config: `config/viewdistance.toml`, registered with `register_configs` so the game writes the
//! defaults when it is missing. The distances the server starts with come from `server.properties`.

pub mod viewdistance_core;
use mod_api::{
    Command, CommandArgumentValue, Config, ConfigKind, ConfigLoadingEvent, ConfigReloadingEvent,
    Flow, HostSide, Hooks, LevelRef, PlayerRef, RegisterNodeCommandsEvent, RegisterConfigsEvent,
    ChunkLoadEvent, PlayerCommandEvent, RunModCommandEvent, ServerAboutToStartEvent, ServerHost, ServerStoppingEvent, ServerTickEvent,
    ServerTickPhase, Suggestion, TabCompleteEvent, Text,
    NodeDefault, PermissionGatherNodesEvent, PermissionLevel, PermissionNode, PermissionValueType,
};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use crate::viewdistance_core::command;
use crate::viewdistance_core::{Change, Distances, Event, Observation, PlayerSample};

const ID: &str = "viewdistance";
const CONFIG_FILE: &str = "viewdistance.toml";
/// The reworded answers (the original's `messages.yml`), next to the config; written with the defaults when missing.
const MESSAGES_FILE: &str = "viewdistance-messages.yml";

/// Reads `config/viewdistance-messages.yml` (writing the default file first when there is none) as the messages in force.
fn load_messages(host: &dyn ServerHost) {
    let path = host.instance_dir().join("config").join(MESSAGES_FILE);
    if !path.exists() {
        let _ = std::fs::create_dir_all(path.parent().unwrap_or_else(|| std::path::Path::new(".")));
        let _ = std::fs::write(&path, crate::viewdistance_core::messages::DEFAULT_YAML);
    }
    if let Ok(text) = std::fs::read_to_string(&path) {
        crate::viewdistance_core::messages::load(&text);
    }
}
const OVERWORLD: &str = "minecraft:overworld";
/// The level the original asks for its fixed-distance nodes: a permissions mod must grant them,
/// the operator fallback stays out (no arismoko operator level reaches 5).
const FIXED_LEVEL: u8 = 5;

/// Chunks generated (not read from the save) since the server started, counted in `chunk_load`.
static GENERATED: AtomicU64 = AtomicU64::new(0);

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
    for problem in crate::viewdistance_core::load_config(text) {
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

/// The typed node for one `viewdistancetweaks.*` name (a Boolean, as the commands ask it).
fn perm_node(name: &str) -> PermissionNode {
    PermissionNode::new(name, PermissionValueType::Boolean)
}

/// Whether `player` holds `node`: the provider's answer, else the node's declared default (the operator level, see
/// `permission_gather_nodes`). An undeclared node or an error is a denial. `_level` is that default, kept for the callers.
fn grants(host: &mut dyn ServerHost, player: PlayerRef, node: &str, _level: u8) -> bool {
    player.permission_bool(host, &perm_node(node)).unwrap_or(false)
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
fn observation(host: &dyn ServerHost, players: &[PlayerRef], loaded: u32, with_chunks: bool) -> Observation {
    let (mspt, tps) = tick_figures(host);
    let player_chunks = if with_chunks { player_chunks(host, players) } else { Vec::new() };
    Observation {
        players: players.len() as u32,
        mspt,
        samples_ms: tick_samples_ms(host),
        loaded_chunks: loaded,
        generated_total: GENERATED.load(Ordering::Relaxed),
        free_memory_percent: crate::viewdistance_core::free_memory_percent(),
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
        crate::viewdistance_core::tick(base, || observation(view, &players, loaded, true))
    };
    if let Some(c) = change.as_ref() {
        let applied = apply_world_distances(host, c);
        if crate::viewdistance_core::log_changes() {
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
    // HMOD-GAP(player-latency): Horizon main has no player ping, so the ping throttle sees 0 ms and never lowers a player.
    // HMOD-GAP(fixed-distance-nodes): `<prefix><n>` (n 2 to 32) is not declared at gather time (its prefix comes from the
    // config and the node's default would have to be "nobody", which a permission level cannot say). Undeclared, it is
    // refused: the per-player fixed view and simulation distances are not applied on Horizon main.
    let samples: Vec<PlayerSample> = if crate::viewdistance_core::personal_due() {
        players
            .iter()
            .map(|p| PlayerSample {
                id: p.uuid(),
                idle_ticks: 0,
                ping_ms: 0,
                fixed_view: None,
                fixed_sim: None,
            })
            .collect()
    } else {
        Vec::new()
    };
    let mut personal = crate::viewdistance_core::personal(|| samples);
    if change.as_ref().is_some_and(|c| c.to.view != c.from.view) {
        // The world's distance was set again, so everyone is back on it.
        personal.extend(crate::viewdistance_core::throttled().into_iter().map(|(id, d, _)| (id, Some(d))));
    }
    apply_personal(host, &personal);

    // Notices go to the players who hold `viewdistancetweaks.notify` (operators of `notify-min-permission`).
    let events = crate::viewdistance_core::take_events();
    if !events.is_empty() {
        let notify_level = crate::viewdistance_core::permissions().0.max(1);
        let notified: Vec<PlayerRef> = players
            .iter()
            .copied()
            .filter(|p| grants(&mut *host, *p, command::NOTIFY_NODE, notify_level))
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
                    for command in crate::viewdistance_core::relief_start(rain > 0.2, thunder > 0.9, advance) {
                        let _ = host.commands().run_command(&command);
                    }
                    log("WARN emergency relief on");
                }
                Event::ReliefEnd => {
                    for command in crate::viewdistance_core::relief_end() {
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
        crate::viewdistance_core::diagnostics(&dir, unix, change, mspt, || crate::viewdistance_core::diag::Sample {
            mspt,
            players: players_now,
            loaded_chunks: loaded,
            distances: base,
            free_memory_percent: crate::viewdistance_core::free_memory_percent(),
            relief: crate::viewdistance_core::relief_active(),
            profile: crate::viewdistance_core::profile_name(),
        });
    }
}

#[mod_api::horizon_mod]
impl Hooks for Mod {
    fn permission_gather_nodes(&self, event: &mut PermissionGatherNodesEvent<'_>) {
        // The `/vdt` root node at `command-min-permission`, and the notice node at `notify-min-permission` (as the
        // fallbacks were), and the group nodes of `/vdt` (the original's `viewdistancetweaks.vdt.command.*`, default op).
        let (notify_level, command_level) = crate::viewdistance_core::permissions();
        let _ = event.nodes.add(
            perm_node("viewdistancetweaks.command.vdt"),
            NodeDefault::Level(PermissionLevel::from_level(i32::from(command_level))),
        );
        for node in command::SUB_NODES {
            let _ = event.nodes.add(
                perm_node(node),
                NodeDefault::Level(PermissionLevel::from_level(i32::from(command_level))),
            );
        }
        let _ = event.nodes.add(
            perm_node(command::NOTIFY_NODE),
            NodeDefault::Level(PermissionLevel::from_level(i32::from(notify_level.max(1)))),
        );
    }

    /// The original's tab completion: profile names, distance values, worlds and flags, after `/vdt`.
    fn tab_complete(&self, _: &mut dyn ServerHost, event: &mut TabCompleteEvent<'_>) {
        let line = event.command.trim_start_matches('/');
        let mut words: Vec<&str> = line.split_whitespace().collect();
        if !words.first().is_some_and(|w| command::is_label(w)) {
            return;
        }
        // The word being typed is the last one, empty after a trailing space.
        if line.ends_with(char::is_whitespace) {
            words.push("");
        }
        let profiles = crate::viewdistance_core::profile_names();
        for text in command::complete(&words[1..], &profiles) {
            if !event.suggestions.iter().any(|s| s.text == text) {
                event.suggestions.push(Suggestion::new((event.range[0], event.range[1]), text));
            }
        }
    }

    fn chunk_load(&self, _: &mut dyn ServerHost, event: &mut ChunkLoadEvent<'_>) {
        if event.new {
            GENERATED.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn register_configs(&self, _: HostSide, event: &mut RegisterConfigsEvent<'_>) {
        let mut config = Config::new(ID, ConfigKind::Common, crate::viewdistance_core::DEFAULT_TOML);
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

    fn register_node_commands(&self, _: &mut dyn ServerHost, event: &mut RegisterNodeCommandsEvent<'_>) {
        // Bukkit gates all of /vdt with the plugin.yml `permission:` `viewdistancetweaks.command.vdt` (default
        // op), so every usage is gated by the root node; `player_command` then asks the group node of the
        // sub-command (`viewdistancetweaks.vdt.command.*`) and answers `&cNo permission.` as the original does.
        // The operator level decides when no permissions mod answers.
        let level = crate::viewdistance_core::permissions().1;
        // Both labels: `/vdt` and Bukkit's namespaced `/viewdistancetweaks:vdt` (plugin.yml registers both).
        for usage in command::USAGES.iter() {
            let Some(tail) = usage.strip_prefix("vdt") else { continue };
            for label in command::LABELS {
                let usage = format!("{label}{tail}");
                event.commands.push(Command::op(&usage, level).with_node(&perm_node("viewdistancetweaks.command.vdt")));
            }
        }
    }

    fn player_command(&self, host: &mut dyn ServerHost, event: &mut PlayerCommandEvent<'_>) -> Flow {
        // plugin.yml gives `/vdt` the node `viewdistancetweaks.command.vdt` (default op): a player without it
        // gets Bukkit's no-permission line, not "Unknown command". The node is the provider's answer, else the
        // operator level `command-min-permission`.
        let root = event.command.split_whitespace().next().unwrap_or("").to_ascii_lowercase();
        if !command::is_label(&root) {
            return Flow::Continue;
        }
        let level = crate::viewdistance_core::permissions().1;
        let allowed = host.player(event.player).is_some_and(|p| grants(&mut *host, p, "viewdistancetweaks.command.vdt", level));
        if allowed {
            // The group node of the sub-command, on top of the root node (the original's `requirePermission`).
            if let Some(node) = command::node_of(&event.command) {
                let held = host.player(event.player).is_some_and(|p| grants(&mut *host, p, node, level));
                if !held {
                    load_messages(&*host);
                    event.replies.push(coloured(&*host, &format!("{}{}", crate::viewdistance_core::messages::prefix(), crate::viewdistance_core::messages::t("no-permission", "&cNo permission.", &[]))));
                    event.cancelled = true;
                    return Flow::Cancel;
                }
            }
            return Flow::Continue;
        }
        event.replies.push(Text::literal("\u{a7}cI'm sorry, but you do not have permission to perform this command. Please contact the server administrators if you believe that this is in error."));
        event.cancelled = true;
        Flow::Cancel
    }

    fn run_mod_command(&self, host: &mut dyn ServerHost, event: &mut RunModCommandEvent<'_>) -> Flow {
        if !event.command.literals.first().is_some_and(|w| command::is_label(w)) {
            return Flow::Continue;
        }
        let present: Vec<(String, String)> = event
            .command
            .arguments
            .iter()
            .filter_map(|a| argument_text(&a.value).map(|t| (a.name.clone(), t)))
            .collect();
        // The usages are written with `vdt`; a namespaced label is matched as that one.
        let mut literals = event.command.literals.clone();
        literals[0] = "vdt".to_owned();
        let Some(line) = typed_line(&literals, &present) else {
            return Flow::Continue;
        };
        load_messages(&*host);
        let base = self.live(&*host);
        let players = host.players().server_players();
        let loaded = loaded_chunks(&*host);
        let obs = observation(&*host, &players, loaded, false);
        let Some(reply) = crate::viewdistance_core::command(&line, base, &obs) else {
            return Flow::Continue;
        };
        let mut lines = reply.lines;
        if reply.reload {
            let path = host.instance_dir().join("config").join(CONFIG_FILE);
            match std::fs::read_to_string(&path) {
                Ok(text) => {
                    lines.push(crate::viewdistance_core::messages::t("reloaded", "&aConfiguration reloaded.", &[]));
                    lines.extend(crate::viewdistance_core::load_config(&text).into_iter().map(|p| format!("&e{p}")));
                }
                Err(e) => lines.push(format!("&c{e}")),
            }
        }
        if let Some(d) = reply.diagnose.as_ref() {
            let unix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
            let dir = host.world_dir().unwrap_or_default().join("viewdistance").join("diagnostics");
            lines.extend(crate::viewdistance_core::diagnose(d, &dir, unix));
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
        for command in crate::viewdistance_core::relief_end() {
            let _ = host.commands().run_command(&command);
        }
    }
}
