fn vdt_register_commands(commands: &mut Vec<horizon_client::mod_api::NativeCommand>) {
    let level = viewdistance_core::permissions().1;
    for (usage, node) in viewdistance_core::command::USAGES.into_iter().zip(viewdistance_core::command::NODES) {
        commands.push(horizon_client::mod_api::NativeCommand::op(usage, level).permission(node));
    }
}
