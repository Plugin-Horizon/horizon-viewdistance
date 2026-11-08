# Gaps against ViewDistanceTweaks 2.15.0

Sources: the jar's `plugin.yml`, `config.yml` and `messages.yml` (data files only; no `.class` file was read),
and the public Hangar page for the command list. Defaults and texts below are the jar's, unless marked.

## Commands

Not ported (no command, no usage, no help line):

- `/vdt analyze`: suggestions from the history. Pure logic; not written in this pass.
- `/vdt doctor`: one-shot check of the setup. Not written.
- `/vdt preset <name>`: the preset values are not in `plugin.yml`, `config.yml` or `messages.yml` (only the
  `{presets}` placeholder is). They are in the plugin's code, which was not read. Cannot be ported from the data files.
- `/vdt pregen ...`: terrain pre-generation (its messages are in `messages.yml`). Needs a chunk generation hook;
  not written.
- `/vdt update [download]`: update checker and auto-update. Not ported (no network access from the mod).

Partly ported:

- `/vdt diagnose status`: not implemented (the `diagnose.status` message needs today's file size and entry count,
  which are not counted yet). It is no longer in the help line or in the usages, since it was advertised but never
  dispatched.
- `/vdt diagnose send|delete|reports`: not ported (report upload is not part of this port).

Ported in this pass: `status --weight` (the chunk weight), `status --fixed` texts, `mspt debug`, the message texts
of all implemented commands, `history` keeps 20 changes.

## Permission nodes

- Ported: `viewdistancetweaks.command.vdt`, `viewdistancetweaks.vdt.command.{reload,status,simulate,profile,viewdistance,simulationdistance}`
  and `viewdistancetweaks.notify`. Commands use `NativeCommand::permission`; the chat notices use
  `native_has_permission` (`tick.rs`). Both fall back to the operator level when no permissions mod answers.
- Per-usage nodes: the host keeps one node per root command (the first usage's, `viewdistancetweaks.command.vdt`), so
  `command.rs` checks each sub-command against its own node (`command::node_of`, `native_has_permission` with the
  sender bound as `Some(player.uuid)` in the `perform-command` injection; the console passes `None` and may do all).
  Unit tested for the node lookup; the injection is not built.
- `native_has_permission` is `pub(super)` on `Game`, which the injected code in `tick.rs` and `command.rs` relies on to
  reach it (check at build time); `NativeCommand::permission` needs the `command_permission` hook assigned at merge.
- Not ported: `viewdistancetweaks.vdt.command.{preset,pregen}` (their commands are missing too);
  nothing else is left out of the nodes list: `viewdistancetweaks.fixed.<n>` and `.simfixed.<n>` are ported (below).

## Config keys

Read from the file but not acted on (ignored; a key missing from the list below is not read at all):

- `world-settings.default.simulation-distance.exclude`, `world-settings.default.view-distance.exclude`,
  `never-reduce-below-players`, `priority-world`: one world only, no effect here.
- `afk-player-throttling.use-essentials-if-present`: no Essentials on Horizon; the AFK check uses the server's idle timer.
- `diagnostics.per-world`, `diagnostics.worlds`, `diagnostics.compress-old-files`: one world; files are not compressed.

Not in the port at all:

- `version`, `update-notify`, `auto-update.*`: update checker, not ported.
- `idle-pregeneration.*`, `predictive-preloading.*`: not ported.
- `per-world-mspt.*`: one world, not ported.
- `client-view-distance-cap.*`: not ported.
- `proactive-mode-settings.use-complexity-weighting`, `complexity-sample-chunks`, `complexity-default-weight`,
  `complexity-weights`: needs chunk block data; not ported.
- `reactive-mode-settings.mspt-tracker-settings.collection-period` and `mspt-variance.sample-window`: the port uses
  the server's last 100 tick times (`Game::native_tick_times`, vanilla's index layout, sorted) for the variance; a window of N needs the order. Not ported.

Removed in this pass: `chunk-generation-safeguard.min-mspt` (it was on the public page, not in the 2.15.0 file).

## Defaults

The defaults in `core/default.toml` and the fallbacks in `core/src/config.rs` are the jar's `config.yml` values,
checked key by key (the live server copy differs in twelve places: that copy is not a default). `profiles.event-mode`
and `profiles.night-mode` are the jar's examples.

## Message texts

Taken from `messages.yml`: reload, enable, disable, status header and summary, variance, startup grace, world line,
weight, simulate (all lines), override (all lines), profile, history, mspt (all but the debug line), help lines of the
implemented commands, diagnose on/off/note.

Still different, on purpose or not verified:

- `/vdt mspt`: the sample source line says "the server's tick times" (the jar says "Paper getTickTimes"); the samples are `Game::native_tick_times`, TPS is `Game::native_tps()[0]`, loaded chunks `Game::native_level_stats("minecraft:overworld").chunks`.
- `/vdt mspt debug`: "Raw samples (ms, oldest first)" is not a message of the jar; the jar has no raw-sample line.
- `/vdt status`: "Free memory" (system `MemAvailable`) where the jar says "Free heap ... MB live / MB max" (heap numbers
  are not available here).
- `/vdt status`: "(via idle timer)" is the source text for AFK throttling; the jar fills `{source}` with its own
  source name. Invented wording.
- `/vdt status --fixed`: the entry line shows the player's UUID, the jar shows the name (`{player}`).
- `/vdt diagnose`: the usage line `[on|off|note <text>]` is not in the jar; `diagnose note` without text uses
  `diagnose.note-usage`. The folder shown is `viewdistance/diagnostics` (relative to the world).
- `/vdt viewdistance` `world` argument: ignored (one world); the jar's messages name the world.
- `/vdt history`: the ago format (`5m ago`) is not in the jar; the jar's `{ago}` placeholder is filled by its own code.
- `/vdt status`: the grace line uses whole seconds from the remaining ticks; the jar's `{seconds}` may round differently.
- `/vdt`: the bare command and `/vdt help` show the help lines of this port (help is not an unknown-command reply
  in the jar; its text is not known).
- The range `2..=32` of `/vdt viewdistance` and `/vdt simulationdistance`: the jar's `invalid-range` message takes its min and max from code; 2 to 32 follows the fixed-distance range in `plugin.yml`.

## Horizon hooks

Nothing needed was missing from the Horizon hook set for the ported items. The missing items above need engine work
or chunk data.

## Tests not run

Nothing was built or run (per the swarm brief). The unit tests changed with the defaults (config, command, grace).
Engine tests that rely on the old defaults (bounds 4/6/10/12, steps 1/1, 30 checks) were not re-derived; they set
their own values in most cases but a build and run is needed.

## Pass 3: checked against the 2.15.0 source (SWARM3)

Reference: the official source, https://codeberg.org/LucasTHCR/ViewDistanceTweaks, licence GPL-3.0, commit `a236c49`
(its `config.yml`, `messages.yml` and `plugin.yml` match the 2.15.0 jar; the 31 top-level classes match too). Read
and compared by hand; nothing was built.

Fixed in `core/src/command.rs` (texts and behaviour taken from `VdtCommand.java` and `messages.yml`):

- `/vdt simulate <world>`: an unknown world answers `&cWorld not found: <world>`; the header names the world or
  `(all worlds)`; the arrow and the new values are coloured as in `simulate.entry`.
- `/vdt viewdistance|simulationdistance`: `--duration` with no positive number means permanent (`parseInt(..., 0)`);
  an unknown world name answers `world-not-found`; `simulationdistance` above the current view raises the view with
  the original's message and the same duration.
- `/vdt mspt`: the sample count, min/max and last ten values only under `mspt debug`; the last ten are space
  separated, oldest first. The "raw samples" line that was not in the original is gone.
- `/vdt history`: ages as the original's `formatAgo` (`1h 5m ago`, `2d 3h ago`).
- `/vdt status`: variance has the three colour tiers (red, yellow above 60% of the threshold, green); the profile
  line only while a profile other than `normal` is active; the memory line is labelled `Free heap`.
- `/vdt help`: the diagnose line text matches `messages.yml` (without `status`, see below).

Still different from the original (found in this pass, not fixed):

- `/vdt diagnose` with no argument, `status` or an unknown word: the original prints the active/inactive line and,
  when active, the status line (today's file, size, entries, interval). The port answers with the usage line
  (`command.rs`, the `diagnose` arm of `run`). Needs the diagnostics state in the port.
- `/vdt diagnose on`: the original adds the status line after the active line. Not ported (see above).
- `/vdt status`: the memory line lacks `(used MB live / max MB)`; the port has no heap figures in `Observation`.
- `/vdt status`: the AFK source is `idle timer`; the original says `EssentialsX` or `Paper idle timer`. Horizon has no
  Essentials, so the wording is the port's guess.
- `/vdt status`: the `Ticking`/`Non-Ticking` counts are the formula (2r+1)^2 per player, not the loaded chunks the
  original counts.
- `/vdt status --fixed`: entries show the player's UUID, not the name, and the `viewdistancetweaks.fixed.<n>` hint
  line is missing (per-player fixed distances are not ported).
- `/vdt mspt`: the original's per-world section is missing (one world).
- `/vdt mspt`, `/vdt status`: the original shows the variance lines only when `mspt-variance.enabled` is true; the
  port shows them always. Guess: the port does not read that switch for the display.
- `/vdt simulate` and the overrides use the world name `world`, since Horizon's server has one world; a request for
  another name is refused with `World not found`.
- `/vdt update`, `analyze`, `doctor`, `preset`, `pregen`: still not ported (see the sections above).
- Permission: the base node `viewdistancetweaks.command.vdt` is not checked before each sub-command (see
  "Permission nodes"); the original checks it first.
- Port-only config keys: `command-min-permission` and `notify-min-permission` (`core/default.toml` lines 22-23,
  read in `core/src/config.rs` lines 189-190) are not in the 2.15.0 `config.yml`. They are the port's own knobs
  for the chat notices and the command gate, not defaults of the original.

Config defaults: 70 of the 70 keys that exist in both files have the jar's default (checked by a script that reads
both files; the port's `default.toml` has 72 keys, two of them port-only, see above).

Licence: the port is now GPL-3.0 (`LICENSE` is the original's text, `NOTICE` names the original's copyright holder,
`Cargo.toml` says `GPL-3.0-only`). The earlier MIT licence and the "clean room" statement are gone: the code is
derived from GPL-3.0 source. Whether `GPL-3.0-only` or `GPL-3.0-or-later` is meant is a guess; the original's
README says "version 3" and gives no "or later".

## Pass 4: Horizon hooks that now exist

- `per-player-fixed-view-distance.*` / `per-player-fixed-simulation-distance.*` (`enabled`, `permission-prefix`; off by default):
  a player holding `<prefix><n>` (n 2 to 32, highest wins; asked with `native_has_permission(.., 5)`, so only a permissions mod
  grants it) gets that view distance through `native_set_player_view_distance` and that simulation distance as the
  `SetSimulationDistance` the client is told. A fixed value beats AFK/ping for the part it names. **Limit**: the engine's
  per-player distance is the smallest of the server's, the client's and this limit, so a fixed view above the world's view
  distance has no effect, and a simulation distance above the world's is clamped to it. Permission asks run every 100 ticks.
- Tick times and TPS come from `native_tick_times` / `native_tps` (no more reading the clock); the chunk count from
  `native_level_stats` (an entity walk, so only when a check or a diagnostics sample is due). `Ticking`/`Non-Ticking` in
  `/vdt status` are still the budget's formula, not a measurement: the stats do not split loaded chunks by ticking.
- Tests: the first revision's tests were stale against the 2.15.0 defaults (11 failed); the test helper now states the old
  defaults it was written for, and `cargo test` passes (51 tests).
