use std::collections::BTreeMap;
use std::collections::BTreeSet;

use serde_derive::Deserialize;

use crate::cmd::*;
use crate::error::*;
use crate::flake::*;

/// Locally available packages with available 'pname' and 'version' attributes.
#[derive(Eq, PartialEq, Ord, PartialOrd, Debug)]
pub(crate) struct Package {
    pub(crate) attribute: String,
    pub(crate) name: String,
    pub(crate) pname: String,
    pub(crate) version: String,
}

/// Returns list of all available packages in parsed form.
pub(crate) fn get_packages(
    nixpkgs: &Option<String>,
    nixos_flake: &Flake,
) -> Result<BTreeSet<Package>, OldeError> {
    // Actual command is taken from pkgs/top-level/make-tarball.nix for
    // 'packages.json.br' build. It's used by repology as is.
    let mut cmd: Vec<&str> = vec![
        "nix-env",
        "-qa",
        "--json",
        "--arg",
        "config",
        "import <nixpkgs/pkgs/top-level/packages-config.nix>",
        "--option",
        "build-users-group",
        "\"\"",
    ];
    let mut a = String::new();
    let na: String;
    match nixpkgs {
        None => {
            // In Nixos without flakes `nix-env` should Just Work.
            // But in system with flakes we need to extract `nixpkgs`
            // input and explicitly pass it in. If it fails we just
            // leave things as is.
            let r = run_cmd(&[
                "nix",
                "--extra-experimental-features",
                "nix-command",
                "--extra-experimental-features",
                "flakes",
                "flake",
                "archive",
                nixos_flake.path().as_str(),
                "--json",
            ]);
            // Assume simplest form:
            // { "inputs": { "nixpkgs": {
            //                 "inputs": {},
            //                 "path": "/nix/store/2z...-source"
            //             }
            match r {
                Err(_) => {
                    log::debug!("Failed to fetch flake archive. Not a flake based system?");
                }
                Ok(p_u8) => {
                    #[derive(Deserialize, Debug)]
                    struct Input {
                        path: String,
                    }
                    #[derive(Deserialize, Debug)]
                    struct Archive {
                        inputs: BTreeMap<String, Input>,
                    }

                    let prefetched: Archive = serde_json::from_slice(p_u8.as_slice())?;

                    // TODO: instead of using last in the list consider
                    // instantiating each of nixkogs inputs (and
                    // recurse into inputs' inputs). That way we should
                    // be able to match all possible packages.
                    for (iname, i) in prefetched.inputs {
                        if iname == "nixpkgs" {
                            a = i.path
                        }
                    }

                    if !a.is_empty() {
                        // Assuming flake-based system.
                        na = format!("nixpkgs={a}");
                        cmd.extend_from_slice(&["-I", &na]);
                        cmd.extend_from_slice(&["-f", &a]);
                    }
                }
            }
        }
        Some(p) => {
            na = format!("nixpkgs={p}");
            cmd.extend_from_slice(&["-I", &na]);
            cmd.extend_from_slice(&["-f", p]);
        }
    }
    let ps_u8 = run_cmd(&cmd)?;
    // "ocamlPackages.patch": {
    //   "name": "ocaml5.3.0-patch-3.0.0",
    //   "pname": "ocaml5.3.0-patch",
    //   "version": "3.0.0"
    // },

    #[derive(Deserialize, Debug)]
    struct Available {
        name: String,
        pname: String,
        version: String,
    }

    let ps: BTreeMap<String, Available> = serde_json::from_slice(ps_u8.as_slice())?;

    let r: BTreeSet<_> = ps
        .iter()
        .map(|(attr, a)| Package {
            attribute: attr.clone(),
            name: a.name.clone(),
            pname: a.pname.clone(),
            version: a.version.clone(),
        })
        .collect();

    // Misconfigured nixpkgs, not a NixOS or flake-based system?
    if r.is_empty() {
        return Err(OldeError::EmptyOutput(String::from("nix-env query")));
    }

    Ok(r)
}
