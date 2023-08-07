/// Flake attribute used to construct system
pub(crate) struct Flake<'a> {
    /// Original path provided by user. Examples are:
    ///     /etc/nixos
    flake: &'a str,
}

impl Flake<'_> {
    pub(crate) fn new<'a>(s: &'a str) -> Flake<'a> {
        Flake{flake: s}
    }

    /// The path part of original flake.
    /// Example: for flake /etc/nixos#vm it should be a /etc/nixos.
    /// TODO: not implemented yet. Just returns original argument.
    pub(crate) fn path<'a>(self: &Self) -> String {
        self.flake.to_string()
    }

    /// The attribute of requested system within the flake.
    /// TODO: not implemented yet. Just returns current system.
    pub(crate) fn system_attribute(self: &Self) -> String {
        format!(
            "nixosConfigurations.{}.config.system.build.toplevel.drvPath",
            gethostname::gethostname()
                .into_string()
                .expect("valid hostname"))
    }
}
