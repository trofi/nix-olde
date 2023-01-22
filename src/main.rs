use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool,Ordering};

use clap::Parser;
use serde_derive::Deserialize;

mod error;
use crate::error::*;
mod cmd;
use crate::cmd::*;

mod repology;
mod installed;

/// Locally available packages with available 'pname' and 'version' attributes.
#[derive(Eq, PartialEq, Ord, PartialOrd)]
struct LocalAvailablePackage {
    attribute: String,
    name: String,
    pname: String,
    version: String,
}

/// Returns list of all available packages in parsed form.
fn get_local_available_packages(nixpkgs: &Option<String>)
    -> Result<BTreeSet<LocalAvailablePackage>, OldeError> {
    // Actual command is taken from pkgs/top-level/make-tarball.nix for
    // 'packages.json.br' build. It's used by repology as is.
    let mut cmd: Vec<&str> = vec![
        "nix-env",
        "-qa",
        "--json",
        "--arg", "config", "import <nixpkgs/pkgs/top-level/packages-config.nix>",
        "--option", "build-users-group", "\"\"",
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
                "--extra-experimental-features", "nix-command",
                "--extra-experimental-features", "flakes",
                "flake", "archive", "/etc/nixos", "--json"]);
            // Assume simplest form:
            // { "inputs": { "nixpkgs": {
            //                 "inputs": {},
            //                 "path": "/nix/store/2z...-source"
            //             }
            match r {
                Err(_) => {
                    // Not a flake-based system? TODO: when verbose dump
                    // here the error to ease debugging.
                },
                Ok(p_u8) => {

                    #[derive(Deserialize)]
                    struct Input { path: String }
                    #[derive(Deserialize)]
                    struct Archive { inputs: BTreeMap<String, Input> }

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
                },
            }
        },
        Some(p) => {
            na = format!("nixpkgs={p}");
            cmd.extend_from_slice(&["-I", &na]);
            cmd.extend_from_slice(&["-f", &p]);
        }
    }
    let ps_u8 = run_cmd(&cmd)?;
    // "nixos.python310Packages.networkx": {
    //   "name": "python3.10-networkx-2.8.6",
    //   "pname": "python3.10-networkx",
    //   "version": "2.8.6"
    // },

    #[derive(Deserialize)]
    struct Available { name: String, pname: String, version: String }

    let ps: BTreeMap<String, Available> =
        serde_json::from_slice(ps_u8.as_slice()).expect("valid json");

    let r: BTreeSet<_> = ps.iter().filter_map(|(attr, a)|
        Some(LocalAvailablePackage{
            attribute: attr.clone(),
            name: a.name.clone(),
            pname: a.pname.clone(),
            version: a.version.clone(),
        })
    ).collect();


    // Misconfigured nixpkgs, not a NixOS or flake-based system?
    if r.is_empty() { return Err(OldeError::EmptyOutput(String::from("nix-env query"))); }

    Ok(r)
}

/// A tool to show outdated packages in current system according to
/// repology.org database.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Alternative path to <nixpkgs> location.
    #[arg(short, long)]
    nixpkgs: Option<String>,

    /// Enable extra verbosity to report unexpected events,
    /// fetch progress and so on.
    #[arg(short, long)]
    verbose: bool,
}

fn main() -> Result<(), OldeError> {
    let o = Args::parse();

    let (r, i, a) = (|| {
       let mut r: Result<BTreeSet::<repology::Package>,       OldeError> =
           Ok(BTreeSet::new());
       let mut i: Result<BTreeSet::<installed::Package>, OldeError> =
           Ok(BTreeSet::new());
       let mut a: Result<BTreeSet::<LocalAvailablePackage>, OldeError> =
           Ok(BTreeSet::new());

       // If an error occured in other 9faster) threads then this
       // flag is raised to signal cancellation.
       let cancel_fetch = AtomicBool::new(false);

       // Each of threads is somewhat slow to proceed:
       // - Repology thread is network-bound
       // - Installed and available threads are CPU-bound
       std::thread::scope(|s| {
         s.spawn(|| {
             eprintln!("Fetching 'repology' ...");
             r = repology::get_packages(o.verbose, &cancel_fetch);
             if r.is_err() {
                 cancel_fetch.store(true, Ordering::Relaxed);
                 eprintln!("... 'repology' failed.");
             } else {
                 eprintln!("... 'repology' done.");
             }
         });
         s.spawn(|| {
             eprintln!("Fetching 'installed' ...");
             i = installed::get_packages(&o.nixpkgs);
             if i.is_err() {
                 cancel_fetch.store(true, Ordering::Relaxed);
                 eprintln!("... 'installed' failed.");
             } else {
                 eprintln!("... 'installed' done.");
             }
         });
         s.spawn(|| {
             eprintln!("Fetching 'available' ...");
             a = get_local_available_packages(&o.nixpkgs);
             if a.is_err() {
                 cancel_fetch.store(true, Ordering::Relaxed);
                 eprintln!("... 'available' failed.");
             } else {
                 eprintln!("... 'available' done.");
             }
         });
       });

       (r, i, a)
    }) ();

    // Report all encountered errors
    if r.is_err() || i.is_err() || a.is_err() {
        let mut errs = Vec::new();
        if r.is_err() { errs.push(r.err().unwrap()) }
        if i.is_err() { errs.push(i.err().unwrap()) }
        if a.is_err() { errs.push(a.err().unwrap()) }

        return Err(OldeError::MultipleErrors(errs));
    }
    let (repology_ps, installed_ps, available_ps) = (r?, i?, a?);

    // 1. go through all local `.drv' files
    // 2. get 'name' from
    // 3. resolve 'name' to 'pname' (via available packages, 'pname' in drv is unstable)
    // 4. match 'pname' against repology database

    // Installed packages not found in 'available'. Should be always empty unless bugs.
    let mut missing_available: Vec<&str> = Vec::new();

    // Package is a local rename of a well known package or repology did not parse it.
    let mut missing_repology: Vec<(&str, &str)> = Vec::new();

    let mut known_versions: BTreeMap<&str, (&Option<String>, BTreeSet<&str>, BTreeSet<&str>)> = BTreeMap::new();

    // TODO: below is a very naive JOIN. Try the alternatives:
    // 1. Map with needed keys.
    // 2. sqlite3's :memory: indices and direct JOIN instead
    for lp in &installed_ps {
        let mut found_in_available = false;

        for ap in &available_ps {
            if lp.name != ap.name { continue }
            found_in_available = true;

            let mut found_on_repology = false;
            for rp in &repology_ps {
                if ap.pname != rp.name { continue }
                found_on_repology = true;

                match known_versions.get_mut(&rp.repology_name as &str) {
                  None => {
                      let mut vs: BTreeSet<&str> = BTreeSet::new();
                      vs.insert(&lp.version);

                      let mut ats: BTreeSet<&str> = BTreeSet::new();
                      ats.insert(&ap.attribute);
                      known_versions.insert(&rp.repology_name, (&rp.latest, vs, ats));
                  },
                  Some((_, ref mut vs, ref mut ats)) => {
                      vs.insert(&lp.version);
                      ats.insert(&ap.attribute);
                  },
                }
            }
            if !found_on_repology {
                missing_repology.push((&ap.pname, &lp.name));
            }
        }
        if !found_in_available {
            missing_available.push(&lp.name);
        }
    }

    let mut found_outdated: isize = 0;
    for (rn, (olv, vs, ats)) in &known_versions {
        if let Some(lv) = olv {
          // Do not print outdated versions if there is use of most recet package
          if vs.contains(&lv as &str) { continue }
        }
        println!("repology {} {:?} | nixpkgs {:?} {:?}",
            rn, (*olv).clone().unwrap_or("<none>".to_string()), vs, ats);
        found_outdated += 1;
    }

    if found_outdated > 0 {
        eprintln!();
        let ratio: f64 = found_outdated as f64 * 100.0 / installed_ps.len() as f64;
        eprintln!("{} of {} ({:.2}%) installed packages are outdated according to https://repology.org.",
                  found_outdated, installed_ps.len(), ratio);
    }

    missing_available.sort();
    missing_repology.sort();
    if o.verbose {
        eprintln!();
        eprintln!("Installed packages missing in available list: {:?}", missing_available);
    } else {
        if !missing_available.is_empty() {
            eprintln!();
            eprintln!("Some installed packages are missing in available list: {}", missing_available.len());
            eprintln!("  Add '--verbose' to get it's full list.");
        }
    }
    Ok(())
}
