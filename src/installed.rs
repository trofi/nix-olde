use std::collections::BTreeMap;
use std::collections::BTreeSet;

use serde_derive::Deserialize;

use crate::cmd::*;
use crate::error::*;

/// Installed packages with available 'pname' and 'version' attributes.
#[derive(Eq, PartialEq, Ord, PartialOrd)]
pub(crate) struct Package {
    /// Full not-quite-'pname' + 'version' from package environment.
    pub(crate) name: String,
    /// 'version' attribute from package environment. Most trusted.
    pub(crate) version: String,
}

fn get_local_system_derivation_via_flakes(nixpkgs: &Option<String>)
    -> Result<String, OldeError> {
    let flake_sys_attr = format!(
        "/etc/nixos#nixosConfigurations.{}.config.system.build.toplevel.drvPath",
        gethostname::gethostname().into_string().expect("valid hostname"));

    let mut cmd: Vec<&str> = vec![
        "nix",
        "--extra-experimental-features", "nix-command",
        "--extra-experimental-features", "flakes",
        "eval",
        // pessimistic case of impure flake
        // TODO: allow passing these flags explicitly when needed
        "--impure",
        "--raw",
        &flake_sys_attr];
    match nixpkgs {
        None => {},
        Some(p) => {
            cmd.extend_from_slice(&["--override-input", "nixpkgs", p]);
        }
    }
    let out_u8 = run_cmd(&cmd)?;
    Ok(String::from_utf8(out_u8).expect("utf8"))
}

fn get_local_system_derivation_via_nixos(nixpkgs: &Option<String>)
    -> Result<String, OldeError> {
    // 'nix eval' could also do here, but it will force a copy. Which
    // takes a few seconds even on SSD. Might be worth it longer term?
    let mut cmd: Vec<&str> = vec![
        "nix-instantiate",
        "<nixpkgs/nixos>",
        "-A", "system"
    ];
    let a: String;
    match nixpkgs {
        None => {},
        Some(p) => {
            a = format!("nixpkgs={p}");
            cmd.extend_from_slice(&["-I", &a]);
        }
    }
    let out_u8 = run_cmd(&cmd)?;
    // Returns path to derivation file (and a newline)
    let out_s = String::from_utf8(out_u8).expect("utf8");
    // Have to drop trailing newline.
    Ok(out_s.trim().to_string())
}

/// Returns store path for local system derivation to later extract
/// all packages used to build it.
fn get_local_system_derivation(nixpkgs: &Option<String>)
    -> Result<String, OldeError> {

    let mut errs = Vec::new();

    // Is there a helper for that?
    let fr = get_local_system_derivation_via_flakes(nixpkgs);
    if fr.is_ok() { return fr; }
    errs.push(fr.err().unwrap());

    let er = get_local_system_derivation_via_nixos(nixpkgs);
    if er.is_ok() { return er; }
    errs.push(er.err().unwrap());

    Err(OldeError::MultipleErrors(errs))
}

/// Returns list of all used derivations in parsed form.
// TODO: add parameters like system expression.
pub(crate) fn get_packages(nixpkgs: &Option<String>)
    -> Result<BTreeSet<Package>, OldeError> {
    let drv_path = get_local_system_derivation(nixpkgs)?;
    let drvs_u8 = run_cmd(&[
        "nix",
        "--extra-experimental-features", "nix-command",
        "show-derivation", "-r", &drv_path])?;
    // {
    //   "/nix/store/...-python3.10-networkx-2.8.6.drv": {
    //     "env": {
    //       "name": "python3.10-networkx-2.8.6",
    //       "pname": "networkx",
    //       "version": "2.8.6"
    //       ...

    #[derive(Deserialize)]
    struct DrvEnv { name: Option<String>, version: Option<String> }
    #[derive(Deserialize)]
    /// Dervivation description with subset of fields needed to detect outdated packages.
    struct Installed { env: DrvEnv, }

    let drvs: BTreeMap<String, Installed> =
        serde_json::from_slice(drvs_u8.as_slice()).expect("valid json");

    let r: BTreeSet<_> = drvs.iter().filter_map(|(_drv, oenv)|
        match &oenv.env {
            DrvEnv{name: Some(n), version: Some(ver)} => Some(
                Package{
                    name: n.clone(),
                    version: ver.clone()
            }),
            // Unversioned derivations. These are usually tarball
            // derivations and tiny wrapper shell scripts with one-off
            // commands.
            _  => None,
        }
    ).collect();

    // Misconfigured system, not a NixOS or flake-based system?
    if r.is_empty() { return Err(OldeError::EmptyOutput(String::from("nix show-derivation"))); }

    Ok(r)
}

