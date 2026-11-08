# Status

Version 0.2.0 (the number is unchanged; this pass changes the mod in place).

Built: no (the bundle was never applied to Horizon). The core crate's unit tests pass (`cargo test` in `mixins/viewdistance/core`, 51 tests).

Changed in this pass, against ViewDistanceTweaks 2.15.0's data files:

- Defaults: the jar's `config.yml` values in `core/default.toml` and in the parser's fallbacks.
- Config names: the world bounds use `world-settings.default.*`; `chunk-weight` added.
- `chunk-weight` scales the world's chunks in the budget; `/vdt status --weight` shows it.
- Permission nodes per usage; `viewdistancetweaks.notify` checked with `native_has_permission`.
- Command tree (this pass): 13 usages became 27, one per sub-command path, with literals (`--fixed`, `--weight`,
  `--duration`, `debug`, `on`, `off`, `note`) and typed arguments (`<value:integer>`, `<world:word>`,
  `<minutes:integer>`, `<text:text>`). `/vdt diagnose status` is removed from the help line (never dispatched).
  Per-usage permission nodes are still not effective (see GAPS.md). Not built.
- Message texts for the implemented commands follow `messages.yml`; help lists the implemented commands.
- `/vdt status --fixed` shows AFK or PING per player; AFK and ping lines with their own counts.
- `startup-grace` off by default; `prioritize-simulation-distance` off; steps 2 and 4.

Commands from the public command list: analyze, doctor, preset, pregen, update not ported; see `GAPS.md`.

## Pass 3 (SWARM3: 1:1 against the source)

- Reference source: https://codeberg.org/LucasTHCR/ViewDistanceTweaks at `a236c49` (GPL-3.0). Its data files and class
  list match the 2.15.0 jar. Details and the list of remaining differences: `GAPS.md`, "Pass 3".
- Licence changed to GPL-3.0: `LICENSE` is the original's text, `NOTICE` added, `Cargo.toml` `license = "GPL-3.0-only"`,
  README "Derived from" line and licence section rewritten.
- `core/src/command.rs`: simulate (world name, colours), pin (`--duration` fallback, world check, simulation pins the
  view), mspt (debug-only samples), history ages, status (variance tiers, profile line, "Free heap"), help diagnose line.
- Tests in `command.rs` updated to the new output; not built (no cargo in this pass).

## Pass 4 (Horizon hooks that now exist)

- Per-player fixed distances by permission (`native_set_player_view_distance`), per-usage `/vdt` permission checks in the
  command, tick times and TPS from `native_tick_times` / `native_tps`, loaded chunks from `native_level_stats`. See
  `GAPS.md`, "Pass 4". Not built against Horizon; the helpers (`tick.rs`, `command.rs`) are the unchecked part.
