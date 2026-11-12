> **Frozen branch (`horizon-plain`).** This is the version written for horizon-rust/horizon's native hooks / horizon-mixins. We stopped working on it on 2026-11-12: horizon-rust moved all mod API work to the fork [arismoko/horizon](https://github.com/arismoko/horizon) (see "Mod-related pull requests" in its CONTRIBUTING.md) and closed mod pull requests there. This branch is experimental and unsupported. The maintained version, for arismoko/horizon only, is on the default branch.

# horizon-viewdistance

> **Unofficial Rust port of ViewDistanceTweaks for the Horizon server (https://github.com/horizon-rust/horizon). Not affiliated with or endorsed by the ViewDistanceTweaks authors.**

A [Horizon](https://github.com/horizon-rust/horizon) native mod that moves a dedicated server's view and
simulation distance with its load: it gives distance back while the server has headroom and takes it away
before players notice lag. It is a rebuild of the idea of the Paper plugin ViewDistanceTweaks, for Horizon's
mod system (horizon-mixins bundle with its own crate).

Derived from ViewDistanceTweaks (https://codeberg.org/LucasTHCR/ViewDistanceTweaks, GPL-3.0), version 2.15.0.
The port follows that source: the commands, messages, defaults and behaviour are taken from it 1:1 where Horizon allows.

Not affiliated with ViewDistanceTweaks or its authors.

## What Horizon already does, and what this adds

Horizon (checked on `69fab597a`, 2026-11-06) sets view and simulation distance once, from
`server.properties`, and lets an operator change them by hand (management API `serversettings/view_distance/set`,
`simulation_distance/set`). The only load-driven distance it has is the *far ring* (`max_view_distance`, saved
chunks past the view): `lan/far.rs` `Brake` pulls that ring in by two chunks a second while the average tick is
over 40 ms. Nothing moves `view_distance` or `simulation_distance` themselves, so this mod is not redundant.

Two engine facts shape what the mod can do (see "Limits" below): the server loads and entity-ticks
every chunk within `view_distance + 1` of a player (`lan_update_streaming`), so on Horizon the view distance is also
the cost knob; and `simulation_distance` is only sent to clients, the server does not use it to limit ticking.

## What it does

Every `ticks-per-check` ticks (after `start-delay`), at the end of a server tick:

- **proactive**: the largest distances whose chunk counts fit the budgets,
  `(2*sim+1)^2 * players <= global-ticking-chunk-count-target` and
  `((2*view+1)^2 - (2*sim+1)^2) * players <= global-non-ticking-chunk-count-target`.
  A server with no players goes to `empty-world-target` (`min`, `max`, `keep`).
- **reactive**: average tick time (the average of the server's last 100 tick times) below `increase-mspt-threshold`
  allows one step up, above `decrease-mspt-threshold` takes one step down. A spread of the last 100 ticks
  above `variance-threshold` (standard deviation, only once the average reaches `min-avg-mspt-to-trigger`) counts as
  overload and blocks increases. A line through the history of (loaded chunks, tick time) predicts the next increase's
  tick time and blocks it when that is over the decrease threshold. An increase that leaves the next check
  overloaded is undone at once (`revert-increase-if-overloaded`).
- **mixed** (default): both, the lower distances win.
- Increases need `passed-checks-for-increase` agreeing checks, decreases `passed-checks-for-decrease`;
  one check moves a distance by at most `max-increase-step` / `max-decrease-step`; results stay inside
  `[world] min-*/max-*`.
- **chunk-generation-safeguard**: more than `chunks-per-second-threshold` newly generated chunks a second blocks
  increases and clamps the view distance to `emergency-view-distance` (below the normal minimum on purpose).
  The clamp is released after `passed-checks-for-increase` checks in a row without generation (my choice: the
  documentation does not say how it ends, and releasing at once made the view flap on a player exploring).
- **memory-safeguard**: below `min-free-percent` of free system memory (`MemAvailable`, Linux) distances only fall.
- **startup-grace**: for the first `duration-ticks` ticks the distances are `startup-grace` values and no check runs.
- `log-changes` writes a console line per change: `[viewdistance] Sim 6 -> 5 | View 12 -> 12 (tick time high)`.

`adjustment-mode = "off"` or `enabled = false` leaves the distances alone.

Added in 0.2.0, **written blind (not built, not run on a server yet)**; the switches that were off on the server this
was built for stay off by default:

- **`/vdt`** for operators of `command-min-permission`, from a player or the console: `reload`, `status [--fixed]`,
  `history` (last 20 changes), `simulate` (what the next check would do, nothing applied), `mspt [debug]` (`debug`
  adds the raw tick samples), `enable` / `disable` (until the next restart), `profile [name]`,
  `viewdistance|simulationdistance <value> [world] [--duration <min>]` (a pin the automatic checks keep, until it runs
  out or the next reload; it works with the mode `off` too), `diagnose on|off|note <text>`. Each usage carries the
  permission node of the real plugin's sub-command group (`viewdistancetweaks.vdt.command.<group>`, or
  `viewdistancetweaks.command.vdt` for `/vdt` and `/vdt help`); a permissions mod can grant them, and without one the
  operator level `command-min-permission` decides. The answer texts are those of the original's `messages.yml`
  (2.15.0) for the sub-commands listed here; see `GAPS.md` for what differs.
- **Profiles** (`[profiles.<name>]`, `/vdt profile <name>`): overlays for the thresholds, steps, checks and chunk
  targets, plus the hard caps `max-view-distance-cap` / `max-simulation-distance-cap`, applied after every other
  rule. The shipped file has the original's two examples; `normal` is the plain config.
- **Chat announcements** (`announce-changes-in-chat`, `announce-format`): to operators of `notify-min-permission`
  (the original's `viewdistancetweaks.notify` defaults to operators; this still checks the operator level only, not
  the node, see `GAPS.md`).
- **Emergency relief** (`[emergency-relief]`): at the minimum distances and still above `decrease-mspt-threshold`,
  `weather clear` and `gamerule advance_time false`; put back exactly as it was below `recover-below-mspt` and when
  the server stops.
- **AFK and ping throttling** (`[afk-player-throttling]`, `[ping-throttling]`): every 5 s; idle for
  `afk-time-minutes` (the server's idle timer) or a ping above the threshold for `checks-before-throttle` samples in a
  row lowers that player's own distances; one good sample lifts the ping throttle. The smaller of both wins.
- **Density-based scaling** (`[density-based-scaling]`): the largest group within `cluster-radius` chunks of one
  player, when at least `cluster-player-threshold`, adds `max-bonus` times its share of all players to the chunk
  budget's distances (my reading of the documentation: the bonus goes to both distances, inside the world range).
- **Diagnostics** (`[diagnostics]`): one JSON line per sample (every `interval-seconds`, or above
  `extra-sample-above-mspt` at most every 10 s), per change and per note, in `<world>/viewdistance/diagnostics/
  <YYYY-MM-DD>.jsonl` (UTC); no player names; `max-file-size-mb` stops samples for the day; files older than
  `keep-days` are deleted. Old files are not compressed (the original packs them as `.jsonl.gz`).

## Config

`config/viewdistance.toml`, written with the defaults when missing, reloaded when the file changes. Keys are
named like the Paper plugin's, with the real `world-settings.default.*` names for the world bounds. The defaults are
the bundled `config.yml` of ViewDistanceTweaks 2.15.0 (`default.toml` in `mixins/viewdistance/core/`); the two
Horizon-only keys `notify-min-permission` and `command-min-permission` are the operator levels for the notify node
and `/vdt`.

## Limits

Not ported:

- **Per-player fixed view and simulation distance by permission** (`viewdistancetweaks.fixed.<n>`, `.simfixed.<n>`; keys
  `per-player-fixed-view-distance.*` and `per-player-fixed-simulation-distance.*`, off by default): ported with
  `native_set_player_view_distance`. The engine's per-player distance is the smallest of the server's, the client's and
  this limit, so a fixed value above the world's distance has no effect; the simulation distance is only what the client
  is told.
- **AFK and ping throttles only lower what a player is sent**: the mod sets the guest's own chunk radius and tells
  their client (`SetChunkCacheRadius`, `SetSimulationDistance`); the server still loads and ticks the world's view
  distance around them, so the freed chunks do not come back to the budget as they do on Paper. The engine resets a
  guest's radius when the world distance changes or the client sends its settings; the mod applies it again at once
  after its own changes and otherwise within 5 s. The helper `set_player_view_distance` does not limit loading either.
- **Per-world MSPT**: Horizon runs one world setting; on one world the feature changes nothing.
- **Idle pre-generation, predictive preloading, the client view-distance cap, complexity weighting**: off on the
  server this was built for; not written.
- `/vdt preset` (its values are not in the data files of the plugin), `/vdt analyze`, `/vdt doctor`, `/vdt pregen`,
  `/vdt update` and the update checker; `use-essentials-if-present`; `never-reduce-below-players`, `priority-world`
  and `exclude` of the world settings (one world only); `sample-window` and `collection-period`. `chunk-weight` is
  ported: it scales the world's chunks in the budget and shows in `/vdt status --weight`. See `GAPS.md`.
Horizon has one world setting for the whole server, so the `world-settings.default` section is the only one read; the nether and the end
are not told apart. Simulation distance reaches the clients only (see above): with
`prioritize-simulation-distance = true` the first steps of a decrease do not lower the server's work; set it to
`false` to move both together.

Assumptions made where the public documentation is silent: when `prioritize-simulation-distance` is on,
increases grow the view first and the simulation last (the reverse of a decrease); the emergency clamp may go
below the configured minimum view distance.

## Build and use

Needs a Horizon source checkout at the commit named in `horizon-mixin.toml` and the `horizon-mixins` tool:

```sh
mkdir -p work/src work/mods
git -C <horizon> archive 69fab597a6f0e8a6ce524d38d1b73e599e343034 | tar -x -C work/src
git -C <horizon> rev-parse 69fab597a6f0e8a6ce524d38d1b73e599e343034 > work/src/.horizon-revision
cp -r mixins/viewdistance work/mods/
horizon-mixins apply --workspace work/src --mods work/mods --out work/stage
cd work/stage && cargo build --profile fast -p horizon-client --no-default-features
```

Unit tests of the decision logic (std only, no Horizon needed):
`cd mixins/viewdistance/core && cargo test`. Everything added in 0.2.0 is covered there only; the glue
(`tick.rs`, `command.rs`, `commands.rs`, `stop.rs`) has not been compiled. Unproven in particular: that
`/vdt` reaches the mod from players and the console, the per-guest radius workaround, `gamerule advance_time` and
`weather` through `native_run_command`, and that commands still run inside `native_server_stopping`.
`tools/distance_probe.py <port> <seconds>` joins a throwaway server as a bare protocol client and prints each
view/simulation distance packet it receives, which is how the change was proven.

## Proof of 0.1.0 (2026-11-06, Horizon `69fab597a`, throwaway dedicated server, `view-distance=12`, `simulation-distance=6`)

`tools/distance_probe.py` joins as a bare protocol client and prints every distance packet. The config file was
swapped while the server ran (`ticks-per-check = 20`, `passed-checks-* = 1`).

| Config swapped in | Engine with the mod: packets the client got |
|---|---|
| calm, `mixed` | view 11, view 12, sim 5, sim 6 (view grows first, simulation last) |
| `proactive`, tiny chunk budget | view 11 + sim 5, then view 10 + sim 4 |
| calm again | view 11, view 12, sim 5, sim 6 |
| `reactive`, thresholds 0.0001 ms | sim 5, sim 4, view 11, view 10; log reason `tick time high` |

The same server and the same sequence on the unmodded engine: no distance packet at all and no log line.
A fresh world with the safeguard on: `View 12 -> 8 (chunk generation)` while the client's chunks were generated, held
until `passed-checks-for-increase` quiet checks, then `View 8 -> 10 (chunk generation over)`. Startup grace logs
`Sim 6 -> 5 | View 12 -> 10 (startup grace)`.

Not proven on a server: the memory safeguard, the variance and prediction rules and a real overload (a throwaway
server with one bare client is never overloaded); they are covered by the unit tests only.

## Licence

GPL-3.0, see `LICENSE` and `NOTICE`. This port is derived from ViewDistanceTweaks 2.15.0
(https://codeberg.org/LucasTHCR/ViewDistanceTweaks, GPL-3.0; copyright LucasTHCR, see `NOTICE`). The original's
source was read at commit `a236c49` of that repository (its `config.yml`, `messages.yml` and `plugin.yml` match
the 2.15.0 jar byte for byte, apart from the version placeholder in `plugin.yml`). Because the port is derived from
GPL-3.0 code, it is GPL-3.0 as well; the earlier MIT licence no longer applies.

## Contributing

Pull requests and improvements are always welcome! Open an issue or a PR -- direct pushes to main are disabled, every change goes through review.
