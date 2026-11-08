fn vdt_tick(game: &mut crate::game::Game<'_>, phase: horizon_client::mod_api::ServerTickPhase) {
    use horizon_client::mod_api::ServerTickPhase;
    use horizon_protocol::packets::play::{self, Clientbound as C};
    use std::sync::atomic::Ordering;
    if !matches!(phase, ServerTickPhase::ServerTickEnd { world_running: true }) {
        return;
    }
    let unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let Some(host) = game.lan.as_ref() else {
        return;
    };
    let actual = viewdistance_core::Distances {
        sim: host.hub.config.simulation_distance,
        view: host.hub.config.view_distance,
    };
    let players = host.hub.guests.iter().filter(|g| g.placed).count() as u32;
    // Each player's dimension (a small number that tells them apart) and chunk, for density.
    let player_chunks: Vec<(u8, i32, i32)> = host
        .hub
        .guests
        .iter()
        .filter(|g| g.placed)
        .map(|g| {
            let dim = g.dimension.bytes().fold(0u8, |h, b| h.wrapping_mul(31).wrapping_add(b));
            (dim, (g.position[0].floor() as i32) >> 4, (g.position[2].floor() as i32) >> 4)
        })
        .collect();
    // The server's last 100 tick times (vanilla's layout) and its ticks a second.
    let (mspt, samples_ms) = viewdistance_core::tick_stats(&game.native_tick_times());
    let tps = game.native_tps()[0].min(20.0);
    let generated_total = game.streaming.generated_total;
    let change = viewdistance_core::tick(actual, || viewdistance_core::Observation {
        players,
        mspt,
        samples_ms,
        // The overworld's loaded chunks (an entity walk, so only when a check is due).
        loaded_chunks: game.native_level_stats("minecraft:overworld").map_or(0, |s| s.chunks as u32),
        generated_total,
        free_memory_percent: viewdistance_core::free_memory_percent(),
        tps: Some(tps),
        player_chunks,
    });
    let Some(host) = game.lan.as_mut() else {
        return;
    };
    if let Some(change) = change {
        if change.to.view != change.from.view {
            host.hub.set_view_distance(change.to.view);
        }
        if change.to.sim != change.from.sim {
            host.hub.set_simulation_distance(change.to.sim);
        }
        if viewdistance_core::log_changes() {
            crate::game::dedicated::log(
                "INFO",
                &format!(
                    "[viewdistance] Sim {} -> {} | View {} -> {} ({})",
                    change.from.sim, change.to.sim, change.from.view, change.to.view, change.reason
                ),
            );
        }
    }
    // Personal (AFK, ping, fixed-by-permission) distances: the view distance is the engine's own
    // per-player limit (`native_set_player_view_distance`: that player is told the radius and is
    // sent chunks within it, at most the world's); the simulation distance has no per-player
    // helper, so only the client is told.
    let ticks = host.hub.ticks;
    let idle: Vec<(u128, u64, i32)> = if viewdistance_core::personal_due() {
        host.hub
            .guests
            .iter()
            .filter(|g| g.placed)
            .map(|g| (g.uuid, ticks.saturating_sub(g.last_action_tick), g.latency.load(Ordering::Relaxed)))
            .collect()
    } else {
        Vec::new()
    };
    // `viewdistancetweaks.fixed.<n>` / `.simfixed.<n>`: the highest n a permissions mod grants (a
    // default level of 5 keeps the operator fallback out, as Bukkit's unset nodes are false). The
    // nodes are asked only when the personal check is due.
    let (view_prefix, sim_prefix) = viewdistance_core::fixed_prefixes();
    let samples: Vec<viewdistance_core::PlayerSample> = idle
        .into_iter()
        .map(|(id, idle_ticks, ping_ms)| viewdistance_core::PlayerSample {
            id,
            idle_ticks,
            ping_ms,
            fixed_view: view_prefix
                .as_deref()
                .and_then(|p| viewdistance_core::fixed_distance(p, |node| game.native_has_permission(id, node, 5))),
            fixed_sim: sim_prefix
                .as_deref()
                .and_then(|p| viewdistance_core::fixed_distance(p, |node| game.native_has_permission(id, node, 5))),
        })
        .collect();
    let mut personal = viewdistance_core::personal(|| samples);
    if change.is_some_and(|c| c.to.view != c.from.view) {
        // set_view_distance gave everyone the world's distance again.
        personal.extend(viewdistance_core::throttled().into_iter().map(|(id, d, _)| (id, Some(d))));
    }
    let Some(host) = game.lan.as_ref() else {
        return;
    };
    let world_sim = host.hub.config.simulation_distance;
    let mut views: Vec<(u128, Option<i32>)> = Vec::new();
    let mut packets: Vec<(u128, C)> = Vec::new();
    for (id, target) in personal {
        if !host.hub.guests.iter().any(|g| g.placed && g.uuid == id) {
            continue;
        }
        let (view, sim) = match target {
            Some(d) => (Some(d.view), d.sim.min(world_sim)),
            None => (None, world_sim),
        };
        views.push((id, view));
        packets.push((id, C::SetSimulationDistance(play::Distance { distance: sim })));
    }
    // Chat notices go to the players the server says hold `viewdistancetweaks.notify` (default: operators
    // of `notify-min-permission`). `host` is not used past this point, so `game` can be asked.
    let notify_level = viewdistance_core::permissions().0;
    let placed: Vec<u128> = host.hub.guests.iter().filter(|g| g.placed).map(|g| g.uuid).collect();
    let notified: Vec<u128> = placed
        .into_iter()
        .filter(|id| game.native_has_permission(*id, viewdistance_core::command::NOTIFY_NODE, notify_level.max(1)))
        .collect();
    for (id, view) in views {
        game.native_set_player_view_distance(id, view);
    }
    for (id, packet) in packets {
        game.native_send(id, packet);
    }
    for event in viewdistance_core::take_events() {
        match event {
            viewdistance_core::Event::Announce(line) => {
                let text = horizon_client::text::Text::legacy(&viewdistance_core::to_section(&line));
                for id in &notified {
                    game.native_tell(*id, text.clone());
                }
            }
            viewdistance_core::Event::ReliefStart => {
                let (rain, thunder) = game.native_weather();
                let advance = game.ui.rules.bool("advance_time");
                for command in viewdistance_core::relief_start(rain > 0.2, thunder > 0.9, advance) {
                    game.native_run_command(&command);
                }
                crate::game::dedicated::log("WARN", "[viewdistance] emergency relief on");
            }
            viewdistance_core::Event::ReliefEnd => {
                for command in viewdistance_core::relief_end() {
                    game.native_run_command(&command);
                }
                crate::game::dedicated::log("INFO", "[viewdistance] emergency relief over");
            }
        }
    }
    if let Some(world) = game.native_world_dir() {
        let dir = world.join("viewdistance").join("diagnostics");
        let now = game
            .lan
            .as_ref()
            .map(|h| viewdistance_core::Distances { sim: h.hub.config.simulation_distance, view: h.hub.config.view_distance })
            .unwrap_or(actual);
        viewdistance_core::diagnostics(&dir, unix, change, mspt, || viewdistance_core::diag::Sample {
            mspt,
            players,
            loaded_chunks: game.native_level_stats("minecraft:overworld").map_or(0, |s| s.chunks as u32),
            distances: now,
            free_memory_percent: viewdistance_core::free_memory_percent(),
            relief: viewdistance_core::relief_active(),
            profile: viewdistance_core::profile_name(),
        });
    }
}
