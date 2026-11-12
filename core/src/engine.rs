//! The decision logic: from what the server looks like every `ticks-per-check` ticks to the view and
//! simulation distance it should run at. No game types, so it is tested on its own.
use crate::config::{Config, EmptyTarget, Mode};
use std::collections::{BTreeMap, VecDeque};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Distances {
    pub sim: i32,
    pub view: i32,
}

/// What a check looks at. Built by the caller only when a check is due.
#[derive(Clone, Debug, Default)]
pub struct Observation {
    pub players: u32,
    /// Average tick time over the last 100 ticks, in milliseconds.
    pub mspt: f64,
    /// Those 100 tick times.
    pub samples_ms: Vec<f64>,
    pub loaded_chunks: u32,
    /// Chunks generated since the server started.
    pub generated_total: u64,
    /// Free share of system memory, when it could be read.
    pub free_memory_percent: Option<f64>,
    /// The server's ticks a second, when the game says (else it is worked out from `mspt`).
    pub tps: Option<f64>,
    /// Each player's dimension (any number that tells them apart) and chunk, for density.
    pub player_chunks: Vec<(u8, i32, i32)>,
}

/// One player, for the personal (AFK, ping) throttles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerSample {
    pub id: u128,
    /// Ticks since the player's last action (the server's idle timer).
    pub idle_ticks: u64,
    pub ping_ms: i32,
    /// The distances the player's `per-player-fixed-*` permission gives them (see [`crate::fixed_distance`]).
    pub fixed_view: Option<i32>,
    pub fixed_sim: Option<i32>,
}

/// What the game should do besides changing the distances.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// A line for the operators who get change notices (`&` colour codes).
    Announce(String),
    /// Emergency relief starts (clear the weather, stop the day cycle) or ends (put them back).
    ReliefStart,
    ReliefEnd,
}

/// A distance pinned with `/vdt viewdistance|simulationdistance`, until a tick or for good.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pin {
    pub value: i32,
    pub until: Option<u64>,
}

#[derive(Clone, Debug, Default)]
struct Personal {
    ping_streak: u32,
    applied: Option<Distances>,
    /// What lowered the distances: "AFK", "PING" or "FIXED" (fixed wins when several do).
    by: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Change {
    pub from: Distances,
    pub to: Distances,
    pub reason: &'static str,
}

#[derive(Clone)]
pub struct Engine {
    /// The config as loaded, and the one in force (with the active profile laid over it).
    base: Config,
    profile: Option<String>,
    cfg: Config,
    /// `/vdt enable|disable`, until the next restart; None follows `enabled` of the config.
    switched: Option<bool>,
    pin_sim: Option<Pin>,
    pin_view: Option<Pin>,
    /// The last changes with their tick, newest last.
    changes: VecDeque<(u64, Change)>,
    events: Vec<Event>,
    relief: bool,
    personal: BTreeMap<u128, Personal>,
    ticks: u64,
    now: Option<Distances>,
    grace_done: bool,
    up_streak: u32,
    down_streak: u32,
    /// The distances before the last increase, while the next check may still undo it.
    undo: Option<Distances>,
    history: VecDeque<(f64, f64)>,
    generated: Option<(u64, u64)>,
    /// The view distance sits at `emergency-view-distance` because chunks were being generated, and the
    /// number of checks in a row without generation since.
    clamped: bool,
    quiet: u32,
}

impl Engine {
    pub fn new(cfg: Config) -> Self {
        Self {
            base: cfg.clone(),
            profile: None,
            cfg,
            switched: None,
            pin_sim: None,
            pin_view: None,
            changes: VecDeque::new(),
            events: Vec::new(),
            relief: false,
            personal: BTreeMap::new(),
            ticks: 0,
            now: None,
            grace_done: false,
            up_streak: 0,
            down_streak: 0,
            undo: None,
            history: VecDeque::new(),
            generated: None,
            clamped: false,
            quiet: 0,
        }
    }

    pub fn config(&self) -> &Config {
        &self.cfg
    }

    /// New settings; the streaks and the tick history start over, the distances, the active
    /// profile (while it still exists) and `/vdt enable|disable` stay. Pins end, as a reload ends
    /// the original's overrides.
    pub fn set_config(&mut self, cfg: Config) {
        let profile = self.profile.take().filter(|p| cfg.with_profile(p).is_some());
        let effective = profile.as_deref().and_then(|p| cfg.with_profile(p)).unwrap_or_else(|| cfg.clone());
        let old = std::mem::replace(self, Self::new(effective));
        self.base = cfg;
        self.profile = profile;
        self.ticks = old.ticks;
        self.now = old.now;
        self.grace_done = old.grace_done;
        self.switched = old.switched;
        self.changes = old.changes;
        self.relief = old.relief;
        self.events = old.events;
        self.personal = old.personal;
    }

    /// Switches to the profile `name` ("normal" = none). False for an unknown name.
    pub fn set_profile(&mut self, name: &str) -> bool {
        let Some(cfg) = self.base.with_profile(name) else {
            return false;
        };
        self.cfg = cfg;
        self.profile = (!name.eq_ignore_ascii_case("normal")).then(|| name.to_ascii_lowercase());
        true
    }

    pub fn profile(&self) -> &str {
        self.profile.as_deref().unwrap_or("normal")
    }

    pub fn base_config(&self) -> &Config {
        &self.base
    }

    /// Whether automatic adjustment runs (`enabled`, or `/vdt enable|disable`).
    pub fn active(&self) -> bool {
        self.switched.unwrap_or(self.cfg.enabled)
    }

    pub fn switch(&mut self, on: bool) {
        self.switched = Some(on);
    }

    /// Pins the simulation (`sim`) or view distance to `value` for `minutes` (None: until the
    /// config is reloaded). Applied at the next tick.
    pub fn pin(&mut self, sim: bool, value: i32, minutes: Option<u64>) {
        let pin = Some(Pin { value, until: minutes.map(|m| self.ticks + m * 1200) });
        if sim {
            self.pin_sim = pin;
        } else {
            self.pin_view = pin;
        }
    }

    pub fn ticks(&self) -> u64 {
        self.ticks
    }

    pub fn history(&self) -> impl Iterator<Item = &(u64, Change)> {
        self.changes.iter()
    }

    pub fn relief_active(&self) -> bool {
        self.relief
    }

    /// The events since the last call.
    pub fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    /// Ticks left of the startup grace, while it runs.
    pub fn grace_left(&self) -> Option<u64> {
        (self.cfg.grace && self.ticks < self.cfg.grace_ticks).then(|| self.cfg.grace_ticks - self.ticks)
    }

    /// Ticks until the next check.
    pub fn next_check_in(&self) -> u64 {
        let c = &self.cfg;
        let every = c.ticks_per_check;
        let first = c.start_delay.max(if c.grace { c.grace_ticks + 1 } else { 0 });
        let next = ((self.ticks / every + 1) * every).max(first.div_ceil(every) * every);
        next - self.ticks
    }

    /// The players whose personal distances are lowered now.
    pub fn throttled(&self) -> Vec<(u128, Distances, &'static str)> {
        self.personal.iter().filter_map(|(id, p)| Some((*id, p.applied?, p.by))).collect()
    }

    pub fn current(&self) -> Option<Distances> {
        self.now
    }

    /// One server tick. `actual` is what the server runs at now (read on the first tick and after
    /// someone else changed it), `observe` builds the observation and is only called for a check.
    /// Returns the distances to apply, when they should change.
    pub fn tick(&mut self, actual: Distances, observe: impl FnOnce() -> Observation) -> Option<Change> {
        let change = self.tick_inner(actual, observe);
        if let Some(c) = change {
            self.record(c);
        }
        change
    }

    fn record(&mut self, c: Change) {
        self.changes.push_back((self.ticks, c));
        while self.changes.len() > 20 {
            self.changes.pop_front();
        }
        if self.cfg.announce {
            let n = |v: i32| v.to_string();
            let line = self
                .cfg
                .announce_format
                .replace("{world}", "world")
                .replace("{oldSim}", &n(c.from.sim))
                .replace("{newSim}", &n(c.to.sim))
                .replace("{oldView}", &n(c.from.view))
                .replace("{newView}", &n(c.to.view));
            self.events.push(Event::Announce(line));
        }
    }

    /// Pinned distances in place of `d`, view never below simulation.
    fn pinned(&self, d: Distances) -> Distances {
        let sim = self.pin_sim.map_or(d.sim, |p| p.value);
        let view = self.pin_view.map_or(d.view, |p| p.value);
        Distances { sim, view: view.max(sim) }
    }

    /// The caps of the active profile, after every other rule.
    fn capped(&self, d: Distances) -> Distances {
        let sim = self.cfg.cap_sim.map_or(d.sim, |c| d.sim.min(c));
        let view = self.cfg.cap_view.map_or(d.view, |c| d.view.min(c));
        Distances { sim: sim.min(view), view }
    }

    fn tick_inner(&mut self, actual: Distances, observe: impl FnOnce() -> Observation) -> Option<Change> {
        self.ticks += 1;
        self.now = Some(actual);
        let now = self.ticks;
        let ended = |p: &mut Option<Pin>| {
            if p.is_some_and(|p| p.until.is_some_and(|u| u <= now)) {
                *p = None;
            }
        };
        ended(&mut self.pin_sim);
        ended(&mut self.pin_view);
        let pinned = self.pinned(actual);
        if pinned != actual {
            return Some(Change { from: actual, to: pinned, reason: "command override" });
        }
        if !self.active() || self.cfg.mode == Mode::Off {
            return None;
        }
        if self.cfg.grace {
            if self.ticks <= self.cfg.grace_ticks {
                if self.grace_done {
                    return None;
                }
                self.grace_done = true;
                let to = self.bounded(Distances { sim: self.cfg.grace_sim, view: self.cfg.grace_view });
                let to = self.pinned(self.capped(to));
                return (to != actual).then_some(Change { from: actual, to, reason: "startup grace" });
            }
        }
        if self.ticks < self.cfg.start_delay || !self.ticks.is_multiple_of(self.cfg.ticks_per_check) {
            return None;
        }
        let change = self.check(actual, &observe());
        self.now = Some(change.map_or(actual, |c| c.to));
        change
    }

    /// What the next check would do with `o`, applied nothing: the distances it aims for, why,
    /// and whether it would already apply them (or still waits for agreeing checks).
    pub fn simulate(&self, cur: Distances, o: &Observation) -> (Distances, &'static str, bool) {
        let mut once = self.clone();
        once.ticks = self.ticks.max(self.cfg.start_delay);
        if let Some(c) = once.check(cur, o) {
            return (c.to, c.reason, true);
        }
        let mut eager = self.clone();
        eager.ticks = once.ticks;
        eager.cfg.passed_checks_for_increase = 1;
        eager.cfg.passed_checks_for_decrease = 1;
        eager.check(cur, o).map_or((cur, "hold", false), |c| (c.to, c.reason, false))
    }

    /// The personal throttles (AFK, ping), every 100 ticks (about 5 s) whatever the mode: the
    /// players whose personal distances change, to the given ones or (None) back to the world's.
    /// Players who are gone are forgotten.
    /// Whether the next [`Engine::personal_tick`] looks at the players: the caller builds its samples
    /// (which may cost permission checks) only then.
    pub fn personal_due(&self) -> bool {
        let c = &self.cfg;
        self.ticks.is_multiple_of(100) && (c.afk || c.ping || c.fixed_view || c.fixed_sim || !self.personal.is_empty())
    }

    pub fn personal_tick(&mut self, observe: impl FnOnce() -> Vec<PlayerSample>) -> Vec<(u128, Option<Distances>)> {
        let c = self.cfg.clone();
        if !self.personal_due() {
            return Vec::new();
        }
        let samples = observe();
        let afk_ticks = (c.afk_minutes * 1200.0) as u64;
        let mut out = Vec::new();
        self.personal.retain(|id, _| samples.iter().any(|s| s.id == *id));
        for s in samples {
            let p = self.personal.entry(s.id).or_default();
            if c.ping && f64::from(s.ping_ms) > c.ping_threshold_ms {
                p.ping_streak += 1;
            } else {
                p.ping_streak = 0;
            }
            let mut target: Option<Distances> = None;
            let mut lower = |sim: i32, view: i32| {
                let d = target.map_or(Distances { sim, view }, |t| Distances { sim: t.sim.min(sim), view: t.view.min(view) });
                target = Some(Distances { sim: d.sim, view: d.view.max(d.sim) });
            };
            let mut by = "";
            if c.afk && s.idle_ticks >= afk_ticks {
                lower(c.afk_sim, c.afk_view);
                by = "AFK";
            }
            if c.ping && p.ping_streak >= c.ping_checks {
                lower(c.ping_sim, c.ping_view);
                by = "PING";
            }
            // A fixed distance beats the throttles; the part it leaves open stays the world's.
            if (c.fixed_view && s.fixed_view.is_some()) || (c.fixed_sim && s.fixed_sim.is_some()) {
                let world = self.now.unwrap_or(Distances { sim: c.max_sim, view: c.max_view });
                let base = target.unwrap_or(world);
                let view = s.fixed_view.filter(|_| c.fixed_view).map_or(base.view, |v| v.clamp(2, 32));
                let sim = s.fixed_sim.filter(|_| c.fixed_sim).map_or(base.sim, |v| v.clamp(2, 32));
                target = Some(Distances { sim, view });
                by = "FIXED";
            }
            p.by = by;
            if target != p.applied {
                p.applied = target;
                out.push((s.id, target));
            }
        }
        out
    }

    fn bounded(&self, d: Distances) -> Distances {
        let c = &self.cfg;
        let sim = d.sim.clamp(c.min_sim, c.max_sim);
        Distances { sim, view: d.view.clamp(c.min_view, c.max_view).max(sim) }
    }

    fn check(&mut self, cur: Distances, o: &Observation) -> Option<Change> {
        let c = self.cfg.clone();
        // Chunks generated per second since the last check.
        let generating = c.gen_safeguard && {
            let rate = self.generated.map_or(0.0, |(total, tick)| {
                let seconds = (self.ticks - tick) as f64 / 20.0;
                o.generated_total.saturating_sub(total) as f64 / seconds
            });
            self.generated = Some((o.generated_total, self.ticks));
            rate > c.gen_chunks_per_second
        };
        let low_memory = c.memory_safeguard && o.free_memory_percent.is_some_and(|f| f < c.memory_min_free_percent);
        // The tick time history for the prediction, kept to `history-length` minutes.
        let keep = (c.prediction_history_minutes * 60 * 20 / c.ticks_per_check).max(2) as usize;
        self.history.push_back((f64::from(o.loaded_chunks), o.mspt));
        while self.history.len() > keep {
            self.history.pop_front();
        }
        let unstable = c.variance
            && o.samples_ms.len() >= c.variance_min_samples
            && o.mspt >= c.variance_min_avg
            && std_dev(&o.samples_ms) > c.variance_threshold;
        let overloaded = o.mspt > c.decrease_mspt || unstable;

        // Emergency relief: nothing left to take from the distances and still over the line.
        if c.relief {
            let floor = cur.sim <= c.min_sim && cur.view <= c.min_view;
            if !self.relief && floor && o.mspt > c.decrease_mspt {
                self.relief = true;
                self.events.push(Event::ReliefStart);
            } else if self.relief && o.mspt < c.relief_recover_mspt {
                self.relief = false;
                self.events.push(Event::ReliefEnd);
            }
        } else if self.relief {
            self.relief = false;
            self.events.push(Event::ReliefEnd);
        }

        // The emergency clamp stays until generation has been quiet for `passed-checks-for-increase`
        // checks in a row, so a player crossing new terrain does not see the distance flap.
        if self.clamped {
            if generating {
                self.quiet = 0;
                return None;
            }
            self.quiet += 1;
            if self.quiet < c.passed_checks_for_increase {
                return None;
            }
            self.clamped = false;
            self.quiet = 0;
            let inside = self.bounded(cur);
            return self.finish(cur, inside, "chunk generation over");
        }

        // An increase that hurt is undone at once, whatever the streaks say.
        if let Some(before) = self.undo.take() {
            if c.revert_increase_if_overloaded && overloaded {
                self.up_streak = 0;
                self.down_streak = 0;
                return self.finish(cur, before, "revert: overloaded after an increase");
            }
        }

        // Out of range (the emergency clamp ended, or the bounds changed): pulled back in at once.
        let inside = self.bounded(cur);
        if inside != cur && !generating {
            self.up_streak = 0;
            self.down_streak = 0;
            return self.finish(cur, inside, "back within the configured range");
        }

        let mut want = cur;
        let mut reason = "hold";
        if c.mode != Mode::Proactive {
            let can_up = o.mspt < c.increase_mspt
                && !(unstable && c.variance_block_increase)
                && !self.prediction_blocks(cur, o);
            if overloaded {
                want = self.step(cur, false, c.max_decrease_step);
                reason = if unstable && o.mspt <= c.decrease_mspt { "tick time unstable" } else { "tick time high" };
            } else if can_up {
                want = self.step(cur, true, c.max_increase_step);
                reason = "tick time low";
            }
        }
        if c.mode != Mode::Reactive {
            let bonus = if c.density { cluster_bonus(&o.player_chunks, c.cluster_radius, c.cluster_threshold, c.max_bonus) } else { 0 };
            let target = self.budget_with(o.players, cur, bonus);
            if c.mode == Mode::Proactive {
                want = target;
            } else {
                want = Distances { sim: want.sim.min(target.sim), view: want.view.min(target.view) };
            }
            if want != cur && reason == "hold" || c.mode == Mode::Proactive {
                reason = "chunk budget";
            }
        }
        // One check moves each distance by at most the step limits.
        want = Distances {
            sim: want.sim.clamp(cur.sim - c.max_decrease_step, cur.sim + c.max_increase_step),
            view: want.view.clamp(cur.view - c.max_decrease_step, cur.view + c.max_increase_step),
        };
        want = self.bounded(want);
        if low_memory {
            reason = "low memory";
            want = Distances { sim: want.sim.min(cur.sim), view: want.view.min(cur.view) };
            if want == cur {
                want = self.step(cur, false, c.max_decrease_step);
            }
        }
        if generating {
            if c.gen_lock_increase {
                want = Distances { sim: want.sim.min(cur.sim), view: want.view.min(cur.view) };
            }
            if c.gen_emergency_view >= 0 && want.view > c.gen_emergency_view {
                want.view = c.gen_emergency_view;
                want.sim = want.sim.min(want.view);
                reason = "chunk generation";
            }
        }

        want = self.pinned(self.capped(want));

        // Increases wait for `passed-checks-for-increase` agreeing checks, decreases for theirs.
        let up = want.sim > cur.sim || want.view > cur.view;
        let down = want.sim < cur.sim || want.view < cur.view;
        if up && !down {
            self.down_streak = 0;
            self.up_streak += 1;
            if self.up_streak < c.passed_checks_for_increase {
                return None;
            }
        } else if down {
            self.up_streak = 0;
            self.down_streak += 1;
            if self.down_streak < c.passed_checks_for_decrease {
                return None;
            }
        } else {
            self.up_streak = 0;
            self.down_streak = 0;
            return None;
        }
        self.up_streak = 0;
        self.down_streak = 0;
        if up && !down {
            self.undo = Some(cur);
        }
        if reason == "chunk generation" && generating && want.view == c.gen_emergency_view {
            self.clamped = true;
            self.quiet = 0;
        }
        self.finish(cur, want, reason)
    }

    fn finish(&mut self, cur: Distances, to: Distances, reason: &'static str) -> Option<Change> {
        (to != cur).then_some(Change { from: cur, to, reason })
    }

    /// `steps` single-chunk moves, simulation distance first when shrinking and last when growing
    /// (it is the costly one) if `prioritize-simulation-distance`, else both together.
    fn step(&self, mut d: Distances, up: bool, steps: i32) -> Distances {
        let c = &self.cfg;
        // A pinned simulation distance is passed over, so the view distance can still move.
        let sim_free = self.pin_sim.is_none();
        for _ in 0..steps {
            if c.prioritize_simulation {
                if up {
                    if d.view < c.max_view {
                        d.view += 1;
                    } else if sim_free && d.sim < c.max_sim {
                        d.sim += 1;
                    }
                } else if sim_free && d.sim > c.min_sim {
                    d.sim -= 1;
                } else if d.view > c.min_view {
                    d.view -= 1;
                }
            } else if up {
                d.sim = (d.sim + 1).min(c.max_sim);
                d.view = (d.view + 1).min(c.max_view);
            } else {
                d.sim = (d.sim - 1).max(c.min_sim);
                d.view = (d.view - 1).max(c.min_view);
            }
        }
        d
    }

    /// The distances whose chunk counts fit the budgets for `players` players.
    #[cfg(test)]
    fn budget(&self, players: u32, cur: Distances) -> Distances {
        self.budget_with(players, cur, 0)
    }

    /// The same, `bonus` chunks more for players standing together.
    fn budget_with(&self, players: u32, cur: Distances, bonus: i32) -> Distances {
        let c = &self.cfg;
        if players == 0 {
            return match c.empty_world {
                EmptyTarget::Min => Distances { sim: c.min_sim, view: c.min_view },
                EmptyTarget::Max => Distances { sim: c.max_sim, view: c.max_view },
                EmptyTarget::Keep => cur,
            };
        }
        // The world's chunk weight scales its chunks in the budget (`world-settings.default.chunk-weight`).
        let n = f64::from(players) * c.chunk_weight;
        let square = |d: i32| f64::from(2 * d + 1).powi(2);
        let sim = (c.min_sim..=c.max_sim).rev().find(|&s| square(s) * n <= c.ticking_target).unwrap_or(c.min_sim);
        let view = (c.min_view..=c.max_view)
            .rev()
            .find(|&v| (square(v) - square(sim)) * n <= c.non_ticking_target)
            .unwrap_or(c.min_view);
        self.bounded(Distances { sim: sim + bonus, view: view + bonus })
    }

    /// Whether a line through the (loaded chunks, tick time) history says one step up would overload.
    fn prediction_blocks(&self, cur: Distances, o: &Observation) -> bool {
        let c = &self.cfg;
        if !c.prediction || self.history.len() < c.prediction_min_samples {
            return false;
        }
        let n = self.history.len() as f64;
        let (sx, sy) = self.history.iter().fold((0.0, 0.0), |(a, b), (x, y)| (a + x, b + y));
        let (mx, my) = (sx / n, sy / n);
        let (mut sxy, mut sxx) = (0.0, 0.0);
        for (x, y) in &self.history {
            sxy += (x - mx) * (y - my);
            sxx += (x - mx) * (x - mx);
        }
        if sxx <= f64::EPSILON {
            return false;
        }
        let slope = sxy / sxx;
        // One step up grows every player's square of chunks.
        let ratio = f64::from(2 * (cur.view + 1) + 1).powi(2) / f64::from(2 * cur.view + 1).powi(2);
        let after = f64::from(o.loaded_chunks) * ratio;
        let predicted = my + slope * (after - mx);
        predicted > c.decrease_mspt
    }
}

/// Density bonus: the largest group of players within `radius` chunks (square distance) of one
/// of them, in one dimension; at least `threshold` of them earn `max_bonus` times their share of
/// all players, rounded.
pub fn cluster_bonus(players: &[(u8, i32, i32)], radius: i32, threshold: u32, max_bonus: i32) -> i32 {
    if players.is_empty() {
        return 0;
    }
    let biggest = players
        .iter()
        .map(|(d, x, z)| {
            players.iter().filter(|(d2, x2, z2)| d2 == d && (x2 - x).abs() <= radius && (z2 - z).abs() <= radius).count()
        })
        .max()
        .unwrap_or(0);
    if (biggest as u32) < threshold {
        return 0;
    }
    (f64::from(max_bonus) * biggest as f64 / players.len() as f64).round() as i32
}

pub fn std_dev(samples: &[f64]) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let n = samples.len() as f64;
    let mean = samples.iter().sum::<f64>() / n;
    (samples.iter().map(|s| (s - mean).powi(2)).sum::<f64>() / n).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tests were written for the first revision's defaults (30 calm checks before an increase,
    /// the world within sim 4 to 6 and view 10 to 12); the file's own values come first and win.
    const OLD_DEFAULTS: &str = "passed-checks-for-increase = 30\n";
    const OLD_WORLD: &str = "[world-settings.default.simulation-distance]\nmin-simulation-distance = 4\nmax-simulation-distance = 6\n[world-settings.default.view-distance]\nmin-view-distance = 10\nmax-view-distance = 12\n";

    fn old_config(toml: &str) -> Config {
        let world = if toml.contains("[world-settings") { "" } else { OLD_WORLD };
        let top = if toml.contains("passed-checks-for-increase") { "" } else { OLD_DEFAULTS };
        let steps = if toml.contains("max-increase-step") || toml.contains("max-decrease-step") {
            ""
        } else {
            "[reactive-mode-settings]\nmax-increase-step = 1\nmax-decrease-step = 1\nprioritize-simulation-distance = true\n"
        };
        Config::parse(&format!("{top}{toml}\n{world}{steps}")).0
    }

    fn engine(toml: &str) -> Engine {
        Engine::new(old_config(toml))
    }

    fn calm(players: u32) -> Observation {
        Observation { players, mspt: 10.0, samples_ms: vec![10.0; 100], loaded_chunks: players * 400, ..Default::default() }
    }

    fn hot(players: u32, mspt: f64) -> Observation {
        Observation { mspt, samples_ms: vec![mspt; 100], ..calm(players) }
    }

    /// Runs `ticks` ticks, returning the last change and the distances afterwards.
    fn run(e: &mut Engine, start: Distances, ticks: u64, obs: &Observation) -> (Distances, Vec<Change>) {
        let mut d = start;
        let mut changes = Vec::new();
        for _ in 0..ticks {
            if let Some(c) = e.tick(d, || obs.clone()) {
                d = c.to;
                changes.push(c);
            }
        }
        (d, changes)
    }

    const NO_GRACE: &str = "[startup-grace]\nenabled = false\n";
    const D: Distances = Distances { sim: 5, view: 10 };

    #[test]
    fn grace_sets_the_distances_once_and_holds_until_it_ends() {
        let mut e = engine("[startup-grace]\nenabled = true\nsimulation-distance = 5\nview-distance = 10\n");
        let first = e.tick(Distances { sim: 8, view: 12 }, || unreachable!()).unwrap();
        assert_eq!((first.to, first.reason), (D, "startup grace"));
        // No check happens inside the grace, whatever the load.
        let (_, changes) = run(&mut e, D, 5000, &hot(3, 80.0));
        assert!(changes.is_empty());
    }

    #[test]
    fn nothing_is_checked_before_start_delay() {
        let mut e = engine(NO_GRACE);
        let (_, changes) = run(&mut e, D, 2399, &hot(3, 80.0));
        assert!(changes.is_empty());
        let (_, changes) = run(&mut e, D, 1, &hot(3, 80.0));
        assert_eq!(changes.len(), 1); // tick 2400 is the first check
    }

    #[test]
    fn overload_shrinks_simulation_first_then_view_one_step_per_check() {
        let mut e = engine(NO_GRACE);
        let start = Distances { sim: 6, view: 12 };
        let (_, changes) = run(&mut e, start, 2400 + 200 * 6, &hot(3, 60.0));
        let seen: Vec<_> = changes.iter().map(|c| (c.to.sim, c.to.view)).collect();
        // The first check at tick 2400 acts: sim 6 -> 5 -> 4, then view 12 -> 11 -> 10, then the floor.
        assert_eq!(seen, [(5, 12), (4, 12), (4, 11), (4, 10)]);
    }

    #[test]
    fn an_increase_needs_thirty_calm_checks_and_a_decrease_one() {
        let mut e = engine(&format!("adjustment-mode = \"reactive\"\n{NO_GRACE}"));
        let low = Distances { sim: 4, view: 10 };
        // Checks start at tick 2400; the 30th calm check is at 2400 + 29 * 200.
        let (_, changes) = run(&mut e, low, 2400 + 29 * 200 - 1, &calm(3));
        assert!(changes.is_empty());
        let (d, changes) = run(&mut e, low, 1, &calm(3));
        assert_eq!(changes.len(), 1);
        assert_eq!((d.sim, d.view), (4, 11)); // view first when growing
    }

    #[test]
    fn mixed_mode_follows_the_tighter_chunk_budget() {
        // 40 players: sim 6 would be 169 * 40 = 6760 > 5780, so the budget caps sim at 5 (121 * 40).
        let mut e = engine(&format!("passed-checks-for-increase = 1\n{NO_GRACE}"));
        let (d, _) = run(&mut e, Distances { sim: 6, view: 12 }, 2400, &calm(40));
        assert_eq!(d.sim, 5);
    }

    #[test]
    fn the_budget_matches_the_documented_example() {
        let e = engine("[world-settings.default.simulation-distance]\nmin-simulation-distance = 2\nmax-simulation-distance = 12\n[world-settings.default.view-distance]\nmin-view-distance = 2\nmax-view-distance = 12\n");
        // 20 players: sim 8 -> 289 * 20 = 5780; view 12 -> (625 - 289) * 20 = 6720.
        assert_eq!(e.budget(20, D), Distances { sim: 8, view: 12 });
        // One more player no longer fits sim 8.
        assert_eq!(e.budget(21, D).sim, 7);
    }

    #[test]
    fn an_empty_server_goes_to_the_minimum() {
        let mut e = engine(&format!("passed-checks-for-increase = 1\n{NO_GRACE}"));
        let (d, _) = run(&mut e, Distances { sim: 6, view: 12 }, 2400 + 200 * 20, &calm(0));
        assert_eq!(d, Distances { sim: 4, view: 10 });
    }

    #[test]
    fn a_revert_undoes_an_increase_that_overloaded_the_server() {
        let mut e = engine(&format!("adjustment-mode = \"reactive\"\npassed-checks-for-increase = 1\n{NO_GRACE}"));
        let low = Distances { sim: 4, view: 10 };
        let (up, changes) = run(&mut e, low, 2400, &calm(3));
        assert_eq!(up.view, 11);
        assert_eq!(changes.len(), 1);
        // The next check is overloaded: straight back to what it was, even with a bigger step in the config.
        let (back, changes) = run(&mut e, up, 200, &hot(3, 60.0));
        assert_eq!(back, low);
        assert_eq!(changes[0].reason, "revert: overloaded after an increase");
    }

    #[test]
    fn unstable_ticks_count_as_overload_even_with_a_fine_average() {
        let mut e = engine(&format!("adjustment-mode = \"reactive\"\n{NO_GRACE}"));
        let spiky: Vec<f64> = (0..100).map(|i| if i % 2 == 0 { 10.0 } else { 50.0 }).collect();
        let obs = Observation { mspt: 30.0, samples_ms: spiky, ..calm(3) };
        let (d, changes) = run(&mut e, Distances { sim: 6, view: 12 }, 2400, &obs);
        assert_eq!(d.sim, 5);
        assert_eq!(changes[0].reason, "tick time unstable");
        // The same average with calm samples is no overload.
        let mut e = engine(&format!("adjustment-mode = \"reactive\"\n{NO_GRACE}"));
        let steady = Observation { mspt: 30.0, samples_ms: vec![30.0; 100], ..calm(3) };
        assert!(run(&mut e, Distances { sim: 6, view: 12 }, 2400, &steady).1.is_empty());
    }

    #[test]
    fn chunk_generation_clamps_the_view_and_releases_it_only_after_quiet_checks() {
        let mut e = engine(&format!("passed-checks-for-increase = 3\n{NO_GRACE}"));
        let start = Distances { sim: 5, view: 11 };
        // Tick 2400 sees 0 generated chunks (the baseline); at tick 2600, 10 s later, 200 more.
        run(&mut e, start, 2400, &calm(3));
        let busy = |total| Observation { generated_total: total, ..calm(3) };
        let (d, changes) = run(&mut e, start, 200, &busy(200));
        assert_eq!(d.view, 8);
        assert_eq!(changes[0].reason, "chunk generation");
        // Two quiet checks are not enough, and generation in between starts the count over.
        let (d, changes) = run(&mut e, d, 400, &busy(200));
        assert!(changes.is_empty());
        let (d, changes) = run(&mut e, d, 200, &busy(600));
        assert!(changes.is_empty());
        let (d, changes) = run(&mut e, d, 400, &busy(600));
        assert!(changes.is_empty());
        // The third quiet check in a row puts the view back inside its range.
        let (d, changes) = run(&mut e, d, 200, &busy(600));
        assert_eq!(d.view, 10);
        assert_eq!(changes[0].reason, "chunk generation over");
    }

    #[test]
    fn a_config_that_moves_the_range_pulls_the_distances_in_at_once() {
        let mut e = engine(&format!("passed-checks-for-increase = 30\n{NO_GRACE}"));
        let (d, changes) = run(&mut e, Distances { sim: 6, view: 12 }, 2400, &calm(3));
        assert!(changes.is_empty());
        e.set_config(old_config(&format!("[world-settings.default.simulation-distance]\nmin-simulation-distance = 4\nmax-simulation-distance = 5\n[world-settings.default.view-distance]\nmin-view-distance = 10\nmax-view-distance = 11\n{NO_GRACE}")));
        let (d, changes) = run(&mut e, d, 200, &calm(3));
        assert_eq!(d, Distances { sim: 5, view: 11 });
        assert_eq!(changes[0].reason, "back within the configured range");
    }

    #[test]
    fn low_free_memory_only_lets_distances_fall() {
        let mut e = engine(&format!("passed-checks-for-increase = 1\n{NO_GRACE}"));
        let obs = Observation { free_memory_percent: Some(5.0), ..calm(3) };
        let (d, changes) = run(&mut e, Distances { sim: 5, view: 11 }, 2400, &obs);
        assert!(d.sim < 5);
        assert_eq!(changes[0].reason, "low memory");
        let ok = Observation { free_memory_percent: Some(60.0), ..calm(3) };
        let mut e = engine(&format!("passed-checks-for-increase = 1\n{NO_GRACE}"));
        let (d, _) = run(&mut e, Distances { sim: 5, view: 11 }, 2400, &ok);
        assert!(d.view >= 11);
    }

    #[test]
    fn the_prediction_stops_an_increase_a_trend_says_will_hurt() {
        // Tick time grows with the chunk count: 0.1 ms a chunk. At 460 loaded chunks one more step
        // (view 10 -> 11: x 1.1) predicts 50 ms, above the 47 ms threshold, though 40 ms now is not.
        let low = Distances { sim: 4, view: 10 };
        let mut hist = engine(&format!("adjustment-mode = \"reactive\"\npassed-checks-for-increase = 1\n{NO_GRACE}"));
        for i in 0..8u32 {
            let chunks = 300 + i * 20;
            hist.history.push_back((f64::from(chunks), f64::from(chunks) * 0.1));
        }
        let now = Observation { mspt: 39.0, loaded_chunks: 440, ..calm(3) };
        assert!(hist.prediction_blocks(low, &now));
        let small = Observation { mspt: 20.0, loaded_chunks: 200, ..calm(3) };
        assert!(!hist.prediction_blocks(low, &small));
    }

    #[test]
    fn off_does_nothing() {
        let mut e = engine("adjustment-mode = \"off\"\n");
        assert!(run(&mut e, Distances { sim: 6, view: 12 }, 20000, &hot(3, 90.0)).1.is_empty());
    }

    #[test]
    fn a_pin_overrides_until_it_runs_out_even_when_off() {
        let mut e = engine("adjustment-mode = \"off\"\n");
        e.pin(false, 14, Some(1));
        let start = Distances { sim: 6, view: 12 };
        let c = e.tick(start, || unreachable!()).unwrap();
        assert_eq!((c.to, c.reason), (Distances { sim: 6, view: 14 }, "command override"));
        // Held for a minute (1200 ticks), then left alone (the mode is off).
        let (d, changes) = run(&mut e, c.to, 1300, &calm(3));
        assert!(changes.is_empty());
        assert_eq!(d.view, 14);
        // A view pin below the simulation distance is raised to it.
        e.pin(false, 3, None);
        assert_eq!(e.tick(d, || unreachable!()).unwrap().to, Distances { sim: 6, view: 6 });
    }

    #[test]
    fn an_automatic_check_keeps_a_pinned_distance() {
        let mut e = engine(NO_GRACE);
        e.pin(true, 6, None);
        let (d, changes) = run(&mut e, Distances { sim: 6, view: 12 }, 2400 + 200 * 4, &hot(3, 60.0));
        assert_eq!(d.sim, 6);
        assert!(changes.iter().all(|c| c.to.sim == 6));
        assert!(d.view < 12);
    }

    #[test]
    fn disable_stops_adjustment_and_enable_resumes_it() {
        let mut e = engine(NO_GRACE);
        e.switch(false);
        assert!(run(&mut e, Distances { sim: 6, view: 12 }, 2400, &hot(3, 60.0)).1.is_empty());
        e.switch(true);
        assert_eq!(run(&mut e, Distances { sim: 6, view: 12 }, 200, &hot(3, 60.0)).1.len(), 1);
    }

    #[test]
    fn a_profile_cap_holds_after_every_rule_and_normal_lifts_it() {
        let mut e = engine(&format!("passed-checks-for-increase = 1\n{NO_GRACE}[profiles.event]\nmax-view-distance-cap = 11\n"));
        assert!(e.set_profile("event"));
        assert!(!e.set_profile("party"));
        assert_eq!(e.profile(), "event");
        let (d, _) = run(&mut e, Distances { sim: 6, view: 12 }, 2400, &calm(3));
        assert_eq!(d.view, 11);
        let (d, _) = run(&mut e, d, 200 * 5, &calm(3));
        assert_eq!(d.view, 11);
        assert!(e.set_profile("normal"));
        let (d, _) = run(&mut e, d, 200, &calm(3));
        assert_eq!(d.view, 12);
        // A reload keeps the profile while it still exists.
        e.set_profile("event");
        e.set_config(Config::parse(&format!("{NO_GRACE}[profiles.event]\nmax-view-distance-cap = 10\n")).0);
        assert_eq!((e.profile(), e.config().cap_view), ("event", Some(10)));
    }

    #[test]
    fn changes_are_kept_and_announced_when_switched_on() {
        let mut e = engine(&format!("announce-changes-in-chat = true\n{NO_GRACE}"));
        run(&mut e, Distances { sim: 6, view: 12 }, 2400, &hot(3, 60.0));
        assert_eq!(e.history().count(), 1);
        assert_eq!(
            e.take_events(),
            vec![Event::Announce("&7[VDT] &fworld: Sim &e6&7\u{2192}&a5 &f| View &e12&7\u{2192}&a12".into())]
        );
        let mut quiet = engine(NO_GRACE);
        run(&mut quiet, Distances { sim: 6, view: 12 }, 2400, &hot(3, 60.0));
        assert!(quiet.take_events().is_empty());
    }

    #[test]
    fn relief_starts_at_the_floor_and_ends_below_the_recovery_line() {
        let mut e = engine(&format!("[emergency-relief]\nenabled = true\n{NO_GRACE}"));
        let floor = Distances { sim: 4, view: 10 };
        run(&mut e, floor, 2400, &hot(3, 60.0));
        assert!(e.relief_active());
        assert_eq!(e.take_events(), vec![Event::ReliefStart]);
        // 45 ms is no longer overloaded but not below 40: relief stays.
        run(&mut e, floor, 200, &hot(3, 45.0));
        assert!(e.relief_active());
        run(&mut e, floor, 200, &hot(3, 30.0));
        assert_eq!(e.take_events(), vec![Event::ReliefEnd]);
        // Off by default: never.
        let mut off = engine(NO_GRACE);
        run(&mut off, floor, 2600, &hot(3, 90.0));
        assert!(off.take_events().is_empty());
    }

    #[test]
    fn players_standing_together_earn_a_bonus() {
        let crowd: Vec<_> = (0..6).map(|i| (0u8, i, 0)).chain([(0, 500, 500), (1, 0, 0)]).collect();
        // 6 of 8 within 8 chunks: 4 * 6 / 8 = 3.
        assert_eq!(cluster_bonus(&crowd, 8, 5, 4), 3);
        assert_eq!(cluster_bonus(&crowd, 8, 7, 4), 0);
        assert_eq!(cluster_bonus(&[], 8, 1, 4), 0);
        let e = engine("[world-settings.default.simulation-distance]\nmin-simulation-distance = 2\nmax-simulation-distance = 12\n[world-settings.default.view-distance]\nmin-view-distance = 2\nmax-view-distance = 12\n");
        assert_eq!(e.budget_with(21, D, 1).sim, 8);
    }

    #[test]
    fn afk_and_bad_ping_lower_personal_distances_and_recover() {
        let mut e = engine(&format!("[afk-player-throttling]\nenabled = true\n[ping-throttling]\nenabled = true\n{NO_GRACE}"));
        let idle = PlayerSample { id: 1, idle_ticks: 6000, ping_ms: 20, fixed_view: None, fixed_sim: None };
        let laggy = PlayerSample { id: 2, idle_ticks: 0, ping_ms: 400, fixed_view: None, fixed_sim: None };
        let mut seen = Vec::new();
        for _ in 0..300 {
            e.tick(Distances { sim: 6, view: 12 }, || calm(2));
            seen.extend(e.personal_tick(|| vec![idle, laggy]));
        }
        // AFK at once; bad ping only on its third sample in a row (ticks 100, 200, 300).
        assert_eq!(seen, vec![(1, Some(Distances { sim: 4, view: 6 })), (2, Some(Distances { sim: 4, view: 6 }))]);
        assert_eq!(e.throttled().len(), 2);
        for _ in 0..100 {
            e.tick(Distances { sim: 6, view: 12 }, || calm(2));
            let back = e.personal_tick(|| vec![PlayerSample { ping_ms: 50, ..laggy }]);
            if !back.is_empty() {
                assert_eq!(back, vec![(2, None)]);
            }
        }
        // Player 1 left and is forgotten.
        assert!(e.throttled().is_empty());
    }

    #[test]
    fn simulate_says_what_the_next_check_aims_for_without_changing_anything() {
        let e = engine(NO_GRACE);
        let cur = Distances { sim: 4, view: 10 };
        let (to, reason, now) = e.simulate(cur, &calm(3));
        assert_eq!((to, reason, now), (Distances { sim: 4, view: 11 }, "tick time low", false));
        let (to, _, now) = e.simulate(Distances { sim: 6, view: 12 }, &hot(3, 60.0));
        assert_eq!((to.sim, now), (5, true));
        assert_eq!(e.ticks(), 0);
    }

    #[test]
    fn a_fixed_distance_beats_the_throttles_and_leaves_the_rest_to_the_world() {
        let mut e = engine(&format!(
            "[afk-player-throttling]\nenabled = true\n[per-player-fixed-view-distance]\nenabled = true\n{NO_GRACE}"
        ));
        let world = Distances { sim: 6, view: 12 };
        let fixed = PlayerSample { id: 1, idle_ticks: 0, ping_ms: 20, fixed_view: Some(20), fixed_sim: Some(9) };
        let afk_and_fixed = PlayerSample { id: 2, idle_ticks: 6000, ping_ms: 20, fixed_view: Some(10), fixed_sim: None };
        let mut seen = Vec::new();
        for _ in 0..100 {
            e.tick(world, || calm(2));
            assert_eq!(e.personal_due(), e.ticks() % 100 == 0);
            seen.extend(e.personal_tick(|| vec![fixed, afk_and_fixed]));
        }
        // The simulation fixed value is off in the config, so only the view part counts; the AFK
        // player keeps the AFK simulation distance with the fixed view.
        assert_eq!(seen, vec![(1, Some(Distances { sim: 6, view: 20 })), (2, Some(Distances { sim: 4, view: 10 }))]);
        assert!(e.throttled().iter().all(|(_, _, by)| *by == "FIXED"));
        // The permission taken away: back to the world's (and AFK for the second player).
        let mut back = Vec::new();
        for _ in 0..100 {
            e.tick(world, || calm(2));
            back.extend(e.personal_tick(|| vec![PlayerSample { fixed_view: None, ..fixed }, PlayerSample { fixed_view: None, ..afk_and_fixed }]));
        }
        assert_eq!(back, vec![(1, None), (2, Some(Distances { sim: 4, view: 6 }))]);
    }
}
