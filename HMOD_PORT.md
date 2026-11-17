# hmod port: viewdistance
Round 1
- Fixed: `grants` takes `&mut dyn ServerHost` and asks the typed node with `permission_bool(...).unwrap_or(false)` (src/lib.rs). Both callers pass `&mut *host`.
- Fixed: `with_node` takes `&PermissionNode`; built with `perm_node("viewdistancetweaks.command.vdt")` (`register_commands`, src/lib.rs).
- Fixed: `PlayerRef::latency` does not exist; `ping_ms` is 0 (see gap) (`check`, src/lib.rs).
- Added: `permission_gather_nodes` declares `viewdistancetweaks.command.vdt` at `command-min-permission` and `viewdistancetweaks.notify` at `notify-min-permission` (`.max(1)`), both from `viewdistance_core::permissions()` (src/lib.rs).
- Permission nodes: `viewdistancetweaks.command.vdt` (default: `command-min-permission`), `viewdistancetweaks.notify` (default: `notify-min-permission`, at least 1); both declared in `permission_gather_nodes` (src/lib.rs). The per-usage sub-nodes (`command::NODES`) are not checked by the port and are not declared.
- Gaps:
  - `player-latency`: Horizon main has no player ping. The ping throttle sees 0 ms, so it never lowers a player's distance. Marked in `check` (src/lib.rs).
  - `fixed-distance-nodes`: `<prefix><n>` (n 2 to 32, prefix from config, `viewdistancetweaks.fixed.` / `viewdistancetweaks.simfixed.` by default) cannot be declared, and a Boolean node's default cannot be "nobody" (the old fallback level 5 was never reached). They are refused, so per-player fixed view and simulation distances are not applied. The prefix lookup is removed in `check` (src/lib.rs).
- Unsure:
  - Node defaults are read at gather time from `viewdistance_core::permissions()`; if the config is not loaded by then, the built-in defaults (1, 1) are used.
  - `FIXED_LEVEL` and `grants`'s `_level` parameter are unused now (warnings only).
