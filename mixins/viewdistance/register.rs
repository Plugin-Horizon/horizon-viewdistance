fn vdt_register(configs: &mut Vec<crate::game::native_configs::NativeConfig>) {
    use crate::game::native_configs::{NativeConfig, NativeConfigKind};
    configs.push(
        NativeConfig::new("viewdistance", NativeConfigKind::Common, viewdistance_core::DEFAULT_TOML)
            .file("viewdistance.toml"),
    );
}
