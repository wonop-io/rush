extern crate rush_cli;

#[cfg(test)]
mod tests {
    use rush_cli::vault::{DotenvVault, FileVault, Vault};
    use std::path::PathBuf;

    #[test]
    fn test_vault_types_exist() {
        // This test just verifies that vault types can be referenced
        let _ = std::any::TypeId::of::<DotenvVault>();
        let _ = std::any::TypeId::of::<FileVault>();
        assert!(true, "Vault types exist");
    }
}
