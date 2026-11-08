fn vdt_reload(file: &crate::game::native_configs::NativeConfigFile) {
    if file.mod_id != "viewdistance" {
        return;
    }
    for problem in viewdistance_core::load_config(&file.text) {
        crate::game::dedicated::log("WARN", &format!("[viewdistance] config: {problem}"));
    }
}
