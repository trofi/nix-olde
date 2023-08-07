use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fs;

use serde_derive::Deserialize;

use crate::cmd::*;
use crate::error::*;

/// Locally available packages with available 'pname' and 'version' attributes.
#[derive(Eq, PartialEq, Ord, PartialOrd, Debug)]
pub(crate) struct Package {
    pub(crate) attribute: String,
    pub(crate) name: String,
    pub(crate) pname: String,
    pub(crate) version: String,
}

/// Returns list of all available packages in parsed form.
pub(crate) fn get_packages(nixpkgs: &Option<String>, nixos_flake: &str) -> Result<BTreeSet<Package>, OldeError> {
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
            //
            let config_dir = fs::canonicalize(nixos_flake)?;

            let r = run_cmd(&[
                "nix",
                "--extra-experimental-features",
                "nix-command",
                "--extra-experimental-features",
                "flakes",
                "flake",
                "archive",
                &config_dir.to_string_lossy(),
                "--json",
            ]);
            // Assume simplest form:
            // { "inputs": { "nixpkgs": {
            //                 "inputs": {},
            //                 "path": "/nix/store/2z...-source"
            //             }
            match r {
                Err(_) => {
                    // Not a flake-based system? TODO: when verbose dump
                    // here the error to ease debugging.
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

                    let prefetched: Archive =
                        serde_json::from_slice(p_u8.as_slice()).expect("valid json");

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
    // "nixos.python310Packages.networkx": {
    //   "name": "python3.10-networkx-2.8.6",
    //   "pname": "python3.10-networkx",
    //   "version": "2.8.6"
    // },

    #[derive(Deserialize, Debug)]
    struct Available {
        name: String,
        pname: String,
        version: String,
    }

    let ps: BTreeMap<String, Available> =
        serde_json::from_slice(ps_u8.as_slice()).expect("valid json");

    let r: BTreeSet<_> = ps
        .iter()
        .filter_map(|(attr, a)| {
            Some(Package {
                attribute: attr.clone(),
                name: a.name.clone(),
                pname: a.pname.clone(),
                version: a.version.clone(),
            })
        })
        .collect();

    // Misconfigured nixpkgs, not a NixOS or flake-based system?
    if r.is_empty() {
        return Err(OldeError::EmptyOutput(String::from("nix-env query")));
    }

    Ok(r)
}
