# hmod port: viewdistance
Parity: 59% now / 59% with our PRs (~119/~200 items; method: commands+sub-commands, permission nodes, config keys, message keys and listed behaviours of the original counted, an item blocked by a missing engine API counts as not done). Blind-written, not compiled.

Round 4 (blind, audit pass)
- Added: `/vdt` is registered under both labels, `vdt` and `viewdistancetweaks:vdt` (`command::LABELS`, `command::is_label`; `register_commands`, `player_command`, `run_mod_command`, tab completion). The colon in a literal is untested on the host parser: drop the second label if it is refused.
- Changed: `/vdt status --fixed` takes its entry, view, sim, source and hint lines from the messages.yml keys (`status.fixed.*`) with the original's placeholders.
- Open: the player's name in the fixed entries (the engine only holds the UUID), predictive preloading and complexity weighting (both need a chunk-request or tile-entity host call, see GAPS_UPSTREAM), `/vdt pregen`, `/vdt update`.

Round 3 (100 % round)
- Added: `messages.yml` (file `config/viewdistance-messages.yml`, written with the original's complete defaults when missing, read on every `/vdt` use): nested keys, `prefix`, `{placeholders}`; every answer of enable/disable/status header+summary+fixed/history/simulate/profile/override/reload/no-permission/preset/analyze/doctor reads its key and falls back to the built-in English text (src/viewdistance_core/messages.rs).
- Added: `/vdt preset <small-server|large-network|performance-first>` (original values; applied to the settings in force, held until the next `/vdt reload` since the config file is the server's), `/vdt analyze|analyse` (all six ConfigAnalyzer rules; the chunk figure is the one loaded now, there is no chunk history), `/vdt doctor` (thresholds, limits, memory, enabled), help lines and tab completion for them. `/vdt pregen` and `/vdt update` answer that they are not available.
- Open: `/vdt pregen` (needs a chunk-generation API, see GAPS_UPSTREAM), `/vdt update`, per-world sections and `world-settings.<world>` (Horizon runs one world per server; keys of unported features), namespaced label, ping/idle/per-player distances (no API).
Round 2 (parity audit fixes)
- Fixed (engine.rs): mixed mode takes the lower target while overloaded and the higher one otherwise; reactive steps scale by 1 + floor(5 ms bands) up to the max step; proactive budget rounds like the original and chunk-weight no longer shrinks it; prediction regresses against the implied chunk count, asks at the next sim distance and blocks on `>=`; reload re-reads `enabled`; relief ends when the engine is disabled; emergency view 0 or less is off.
- Fixed (command.rs, lib.rs): group permission nodes `viewdistancetweaks.vdt.command.*` declared and checked (`No permission.`); unknown sub-commands show help; zero-argument usages; bad distance values answer `Invalid value`; `diagnose` status lines and notes only while on; `status --fixed` header/hint; variance lines hidden when off; tab completion.
- Fixed (config.rs): numbers are clamped like the original (ticks-per-check >= 20, distances 2 to 32, max raised to min, ...).
- Closed gap: generated chunks are counted from `chunk_load` (`new`).
- Open: messages.yml, `/vdt analyze|doctor|preset|pregen|update`, per-world settings and the keys of unported features, `/viewdistancetweaks:vdt`, startup-grace/check-cadence details, the chunk-generation safeguard keeps its own release rule.

Round 1
- Fixed: `grants` takes `&mut dyn ServerHost` and asks the typed node with `permission_bool(...).unwrap_or(false)` (src/lib.rs). Both callers pass `&mut *host`.
- Fixed: `with_node` takes `&PermissionNode`; built with `perm_node("viewdistancetweaks.command.vdt")` (`register_commands`, src/lib.rs).
- Fixed: `PlayerRef::latency` does not exist; `ping_ms` is 0 (see gap) (`check`, src/lib.rs).
- Added: `permission_gather_nodes` declares `viewdistancetweaks.command.vdt` at `command-min-permission` and `viewdistancetweaks.notify` at `notify-min-permission` (`.max(1)`), both from `viewdistance_core::permissions()` (src/lib.rs).
- Permission nodes: `viewdistancetweaks.command.vdt` (default: `command-min-permission`), `viewdistancetweaks.notify` (default: `notify-min-permission`, at least 1); both declared in `permission_gather_nodes` (src/lib.rs). The eight group nodes `viewdistancetweaks.vdt.command.{reload,status,simulate,profile,viewdistance,simulationdistance,preset,pregen}` are declared at the same default level and checked in `player_command`.
- Gaps:
  - `player-latency`: Horizon main has no player ping. The ping throttle sees 0 ms, so it never lowers a player's distance. Marked in `check` (src/lib.rs).
  - `fixed-distance-nodes`: `<prefix><n>` (n 2 to 32, prefix from config, `viewdistancetweaks.fixed.` / `viewdistancetweaks.simfixed.` by default) cannot be declared, and a Boolean node's default cannot be "nobody" (the old fallback level 5 was never reached). They are refused, so per-player fixed view and simulation distances are not applied. The prefix lookup is removed in `check` (src/lib.rs).
- Unsure:
  - Node defaults are read at gather time from `viewdistance_core::permissions()`; if the config is not loaded by then, the built-in defaults (1, 1) are used.
  - `FIXED_LEVEL` and `grants`'s `_level` parameter are unused now (warnings only).
