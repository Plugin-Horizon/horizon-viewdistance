// The server stops: weather and the day cycle that emergency relief changed are put back,
// since the game rule is saved with the world.
fn vdt_stop(game: &mut crate::game::Game<'_>) {
    for command in viewdistance_core::relief_end() {
        game.native_run_command(&command);
    }
}
