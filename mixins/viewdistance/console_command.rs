// `/vdt` from a player or the console (`sender` None): the answer goes back through `native_feedback`.
// The host gates a player's `/vdt` with the first usage's node only, so each sub-command is checked
// here against its own node (`viewdistancetweaks.vdt.command.<name>`); operators of
// `command-min-permission` pass where no permissions mod answers.
fn vdt_console_command(game: &mut crate::game::Game<'_>, command: &str, ran: &mut bool) {
    let sender: Option<u128> = None;
    if let (Some(player), Some(node)) = (sender, viewdistance_core::command::node_of(command)) {
        let level = viewdistance_core::permissions().1.max(1);
        if !game.native_has_permission(player, node, level) {
            *ran = true;
            game.native_feedback(horizon_client::text::Text::legacy(&viewdistance_core::to_section(
                "&cI'm sorry, but you do not have permission to perform this command.",
            )));
            return;
        }
    }
    let (players, actual) = game.lan.as_ref().map_or((0, viewdistance_core::Distances { sim: 0, view: 0 }), |h| {
        (
            h.hub.guests.iter().filter(|g| g.placed).count() as u32,
            viewdistance_core::Distances { sim: h.hub.config.simulation_distance, view: h.hub.config.view_distance },
        )
    });
    let (mspt, samples_ms) = viewdistance_core::tick_stats(&game.native_tick_times());
    let observation = viewdistance_core::Observation {
        players,
        mspt,
        samples_ms,
        loaded_chunks: game.native_level_stats("minecraft:overworld").map_or(0, |s| s.chunks as u32),
        free_memory_percent: viewdistance_core::free_memory_percent(),
        tps: Some(game.native_tps()[0].min(20.0)),
        ..Default::default()
    };
    let Some(reply) = viewdistance_core::command(command, actual, &observation) else {
        return;
    };
    *ran = true;
    let mut lines = reply.lines;
    if reply.reload {
        let text = crate::game::Game::native_config_dir()
            .and_then(|dir| std::fs::read_to_string(dir.join("viewdistance.toml")).map_err(|e| e.to_string()));
        match text {
            Ok(text) => {
                lines.push("&aConfiguration reloaded.".into());
                lines.extend(viewdistance_core::load_config(&text).into_iter().map(|p| format!("&e{p}")));
            }
            Err(e) => lines.push(format!("&c{e}")),
        }
    }
    if let (Some(d), Some(world)) = (reply.diagnose, game.native_world_dir()) {
        let unix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        viewdistance_core::diagnose(&d, &world.join("viewdistance").join("diagnostics"), unix);
    }
    for line in lines {
        game.native_feedback(horizon_client::text::Text::legacy(&viewdistance_core::to_section(&line)));
    }
}
