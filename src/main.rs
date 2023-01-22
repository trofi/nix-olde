// TODO: can we move it out to Cargo.toml? Or a separate file?
mod cmd;
mod error;

mod repology;
mod installed;
mod available;

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool,Ordering};

use clap::Parser;

use crate::error::*;

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
       let mut r: Result<BTreeSet::<repology::Package>, OldeError> =
           Ok(BTreeSet::new());
       let mut i: Result<BTreeSet::<installed::Package>, OldeError> =
           Ok(BTreeSet::new());
       let mut a: Result<BTreeSet::<available::Package>, OldeError> =
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
             a = available::get_packages(&o.nixpkgs);
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
