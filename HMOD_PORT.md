# hmod port: viewdistance
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
