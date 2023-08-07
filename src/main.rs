// TODO: can we move it out to Cargo.toml? Or a separate file?
mod cmd;
mod error;
mod flake;
mod opts;
mod progress;

// package loading modules
mod available;
mod installed;
mod repology;

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::error::*;
use crate::flake::*;
use crate::opts::*; // TODO: how to avoid explicit import?
use crate::progress::*;

/// Ideally we would just use flake path as is. In practice we have to
/// dereference symlinks for local paths.
/// TODO: move it's handling to `crate::flake`.
fn resolve_flake(s: String) -> String {
    match std::fs::canonicalize(s.clone()) {
        Err(e) => {
            log::info!("Failed to canonicalize path {s}. Assuming flake syntax.");
            log::debug!("canonicalization failure for {s}: {e}");
            s
        }
        Ok(r) => r.to_string_lossy().to_string(),
    }
}

fn main() -> Result<(), OldeError> {
    let o = Opts::parse();
    env_logger::Builder::new()
        .format(|buf, record| writeln!(buf, "{}: {}", record.level(), record.args()))
        .filter_level(o.verbose.log_level_filter())
        .init();

    // TODO: add support for attribute selection as well:
    //   /etc/nixos#foo or github:foo/bar#baz
    // should both produce full path to an attribute.
    // TODO: add support for more general flake syntax from `nix flake --help`.
    let resolved_path = resolve_flake(o.flake.unwrap_or("/etc/nixos".to_string()));
    let nixos_flake = Flake::new(&resolved_path);

    let (r, i, a) = {
        let mut r: Result<BTreeSet<repology::Package>, OldeError> = Ok(BTreeSet::new());
        let mut i: Result<BTreeSet<installed::Package>, OldeError> = Ok(BTreeSet::new());
        let mut a: Result<BTreeSet<available::Package>, OldeError> = Ok(BTreeSet::new());

        // If an error occured in other (faster) threads then this
        // flag is raised to signal cancellation.
        let cancel_flag = &AtomicBool::new(false);
        let cancel = || {
            cancel_flag.store(true, Ordering::Relaxed);
        };
        let poll_cancel = || cancel_flag.load(Ordering::Relaxed);

        // Each of threads is somewhat slow to proceed:
        // - Repology thread is network-bound
        // - Installed and available threads are CPU-bound
        std::thread::scope(|s| {
            s.spawn(|| {
                let mut p = TaskProgress::new("repology");
                r = repology::get_packages(&poll_cancel);
                if r.is_err() {
                    cancel();
                    p.fail();
                }
            });
            s.spawn(|| {
                let mut p = TaskProgress::new("installed");
                i = installed::get_packages(&o.nixpkgs, &nixos_flake);
                if i.is_err() {
                    cancel();
                    p.fail();
                }
            });
            s.spawn(|| {
                let mut p = TaskProgress::new("available");
                a = available::get_packages(&o.nixpkgs, &nixos_flake);
                if a.is_err() {
                    cancel();
                    p.fail();
                }
            });
        });

        (r, i, a)
    };
    eprintln!();

    // Report all encountered errors
    if r.is_err() || i.is_err() || a.is_err() {
        let mut errs = Vec::new();
        if r.is_err() {
            errs.push(r.err().unwrap())
        }
        if i.is_err() {
            errs.push(i.err().unwrap())
        }
        if a.is_err() {
            errs.push(a.err().unwrap())
        }

        return Err(OldeError::MultipleErrors(errs));
    }
    let (repology_ps, installed_ps, available_ps) = (r?, i?, a?);

    // Installed packages not found in 'available'. Should be always empty.
    // The exceptions are intermediate derivations for scripts and during
    // bootstrap.
    let mut missing_available: Vec<&str> = Vec::new();

    // Packages not found in Repology database. Usually a package rename.
    let mut missing_repology: Vec<(&str, &str)> = Vec::new();

    let mut known_versions: BTreeMap<&str, (&Option<String>, BTreeSet<&str>, BTreeSet<&str>)> =
        BTreeMap::new();

    // Map installed => available => repology. Sometimes mapping is
    // one-to-many.
    for lp in &installed_ps {
        let mut found_in_available = false;

        for ap in &available_ps {
            if lp.name != ap.name {
                continue;
            }
            found_in_available = true;

            let mut found_on_repology = false;
            for rp in &repology_ps {
                if ap.pname != rp.name {
                    continue;
                }
                found_on_repology = true;

                match known_versions.get_mut(&rp.repology_name as &str) {
                    None => {
                        let mut vs: BTreeSet<&str> = BTreeSet::new();
                        vs.insert(&lp.version);

                        let mut ats: BTreeSet<&str> = BTreeSet::new();
                        ats.insert(&ap.attribute);
                        known_versions.insert(&rp.repology_name, (&rp.latest, vs, ats));
                    }
                    Some((_, ref mut vs, ref mut ats)) => {
                        vs.insert(&lp.version);
                        ats.insert(&ap.attribute);
                    }
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
            if vs.contains(lv as &str) {
                continue;
            }
        }
        println!(
            "repology {} {:?} | nixpkgs {:?} {:?}",
            rn,
            (*olv).clone().unwrap_or("<none>".to_string()),
            vs,
            ats
        );
        found_outdated += 1;
    }

    if found_outdated > 0 {
        eprintln!();
        let ratio: f64 = found_outdated as f64 * 100.0 / installed_ps.len() as f64;
        eprintln!(
            "{} of {} ({:.2}%) installed packages are outdated according to https://repology.org.",
            found_outdated,
            installed_ps.len(),
            ratio
        );
    }

    missing_available.sort();
    missing_repology.sort();
    if log::log_enabled!(log::Level::Debug) {
        eprintln!();
        eprintln!(
            "Installed packages missing in available list: {:?}",
            missing_available
        );
    } else if !missing_available.is_empty() {
        eprintln!();
        eprintln!(
            "Some installed packages are missing in available list: {}",
            missing_available.len()
        );
        eprintln!("  Add '--verbose' to get it's full list.");
    }
    Ok(())
}
