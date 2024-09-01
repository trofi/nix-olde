/// Flake attribute used to construct system
pub(crate) struct Flake {
    /// Path to a flake (without an attribute). Examples are:
    ///     /etc/nixos
    ///     github:user/nixos-config
    flake: String,
    /// `nixosConfigurations` or `darwinConfigurations`
    configurations_attribute: String,
    /// System name as an attribute in `nixosConfigurations`.
    name: String,
}

/// Ideally we would just use flake path as is. In practice we have to
/// dereference symlinks for local paths.
pub(crate) fn resolve_flake(s: &str) -> String {
    match std::fs::canonicalize(s) {
        Err(e) => {
            log::info!("Failed to canonicalize path {s}. Assuming flake syntax.");
            log::debug!("canonicalization failure for {s}: {e}");
            s.to_string()
        }
        Ok(r) => r
            .into_os_string()
            .into_string()
            .expect("flake path decoding failure"),
    }
}

impl Flake {
    pub(crate) fn new(s: &Option<String>) -> Flake {
        // Disambiguate 2 forms:
        // 1. with explicit attribute: /etc/nixos#vm
        // 2. without the attribute: /etc/nixos (needs hostname access)

        // TODO: propagate the error up.
        let hostname = gethostname::gethostname()
            .into_string()
            .expect("hostname decoding failure");

        let flake_uri = s.as_deref().unwrap_or("/etc/nixos");
        let (flake, name): (&str, &str) = match flake_uri.split_once('#') {
            None => (&flake_uri, &hostname),
            Some(fln) => fln,
        };

        let configurations_attribute = "nixosConfigurations";
        #[cfg(target_os = "macos")]
        let configurations_attribute = "darwinConfigurations";

        Flake {
            // TODO: try to resolve symlinks for paths in flake syntax
            // like 'git+file:///etc/nixos' (if `nixos-rebuild` supports
            // it).
            flake: resolve_flake(flake),
            name: name.to_string(),
            configurations_attribute: configurations_attribute.to_string()
        }
    }

    /// The path part of original flake.
    /// Example: for flake /etc/nixos#vm it should be a /etc/nixos.
    /// TODO: not implemented yet. Just returns original argument.
    pub(crate) fn path(self: &Self) -> String {
        self.flake.to_string()
    }

    /// The attribute of requested system within the flake.
    /// TODO: not implemented yet. Just returns current system.
    pub(crate) fn system_attribute(self: &Self) -> String {
        format!(
            "{}.{}.config.system.build.toplevel.drvPath",
            self.configurations_attribute,
            self.name
        )
    }
}
