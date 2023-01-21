use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::process::Command;
use std::sync::atomic::{AtomicBool,Ordering};

use clap::Parser;
use serde_derive::Deserialize;
use thiserror::Error;

#[derive(Error, Debug, Clone)]
enum OldeError {
    /// Running external command failed for some reason.
    #[error("command {cmd:?} failed: {output:?}")]
    CommandFailed {
        cmd: Vec<String>,
        output: std::process::Output,
    },

    // Multiple errors happened. See individual entries for an
    // explanation.
    #[error("multiple errors: {0:?}")]
    MultipleErrors(Vec<OldeError>),

    // Cancelled externally.
    #[error("canceled {0}")]
    Canceled(String),

    // Unexpected empty output.
    #[error("unexpected empty output from {0}")]
    EmptyOutput(String),
}

/// Runs 'cmd' and returns stdout or failure.
fn run_cmd(args: &[&str]) -> Result<Vec<u8>, OldeError> {
    let output = Command::new(args[0]).args(&args[1..])
                      .output()
                      .expect("Failed to run command");

    if !output.status.success() {
       return Err(OldeError::CommandFailed {
            cmd: args.into_iter().map(|a| a.to_string()).collect(),
            output,
        });
    }

    Ok(output.stdout)
}

/// Installed packages with available 'pname' and 'version' attributes.
#[derive(Eq, PartialEq, Ord, PartialOrd)]
struct LocalInstalledPackage {
    /// Full not-quite-'pname' + 'version' from package environment.
    name: String,
    /// 'version' attribute from package environment. Most trusted.
    version: String,
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
    // TODO: is there a 'nix' command equivalent? Maybe 'nix eval'?
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
// TODO: add parameters like a directory defining nixpkgs path, a system expression.
fn get_local_installed_packages(nixpkgs: &Option<String>)
    -> Result<BTreeSet<LocalInstalledPackage>, OldeError> {
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
                LocalInstalledPackage{
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


/// Locally available packages with available 'pname' and 'version' attributes.
#[derive(Eq, PartialEq, Ord, PartialOrd)]
struct LocalAvailablePackage {
    attribute: String,
    name: String,
    pname: String,
    version: String,
}

/// Returns list of all available packages in parsed form.
// TODO: add parameters like a directory defining nixpkgs path, a system expression.
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

/// Installed packages with available 'pname' and 'version' attributes.
#[derive(Eq, PartialEq, Ord, PartialOrd)]
struct RepologyPackage {
    /// repology package name
    repology_name: String,

    /// nixpkgs 'pname' from available packages
    name: String,

    version: String,
    /// repology's characterization of the state: outdated, dev-only, etc.
    status: String,

    /// latest version available in some other repository
    /// Might not exist if latest version was added and then
    /// removed from repology.org.
    latest: Option<String>,
}

/// Returns list of all outdated derivations according to repology.
fn get_repology_packages(verbose: bool, cancel_fetch: &AtomicBool)
    -> Result<BTreeSet<RepologyPackage>, OldeError> {
    let mut r = BTreeSet::new();

    // We pull in all package ingo py paginating through
    //     https://repology.org/api/v1/projects/?inrepo=nix_unstable&outdated=1
    //     https://repology.org/api/v1/projects/${suffix}?inrepo=nix_unstable&outdated=1
    let mut suffix: String = "".to_string();

    loop {
        if cancel_fetch.load(Ordering::Relaxed) {
            return Err(OldeError::Canceled(String::from("Repology fetch")));
        }
        let url = format!("https://repology.org/api/v1/projects/{suffix}?inrepo=nix_unstable&outdated=1");

        if verbose {
            eprintln!("Fetching from repology: {:?}", suffix);
        }
        let contents_u8 = run_cmd(&["curl", "--compressed", "-s", &url])?;
        // {
        //   "python:networkx": [
        //     {
        //       "repo": "nix_unstable",
        //       "name": "python3.10-networkx",
        //       "version": "2.8.6",
        //       "status": "outdated",
        //     },

        #[derive(Deserialize, Debug)]
        /// Dervivation description with subset of fields needed to detect outdated packages.
        struct Repology { repo: String, name: Option<String>, version: String, status: String }

        let pkgs: BTreeMap<String, Vec<Repology>> =
            serde_json::from_slice(contents_u8.as_slice()).expect("valid json");

        let mut next_suffix = suffix.clone();
        for (n, vs) in &pkgs {
            next_suffix = n.clone() + "/";

            let olatest_entry =
                vs.iter()
                  .find_map(|e| if e.status == "newest" || e.status == "unique" { Some(e) } else { None });
            let latest = olatest_entry.map(|e| e.version.clone());

            // There can be multiple nix_unstable package entries for a
            // single repology entry: pycropto vs pycryptodome.
            // Store all of them.
            for v in vs {
                if v.repo != "nix_unstable" { continue }

                r.insert(RepologyPackage {
                    repology_name: n.clone(),
                    name: v.name.clone().expect("name"),
                    version: v.version.clone(),
                    status: v.status.clone(),
                    latest: latest.clone(),
                });
            }
        }
        if suffix == next_suffix { break }
        suffix = next_suffix;
    }

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
       let mut r: Result<BTreeSet::<RepologyPackage>,       OldeError> =
           Ok(BTreeSet::new());
       let mut i: Result<BTreeSet::<LocalInstalledPackage>, OldeError> =
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
             r = get_repology_packages(o.verbose, &cancel_fetch);
             if r.is_err() {
                 cancel_fetch.store(true, Ordering::Relaxed);
                 eprintln!("... 'repology' failed.");
             } else {
                 eprintln!("... 'repology' done.");
             }
         });
         s.spawn(|| {
             eprintln!("Fetching 'installed' ...");
             i = get_local_installed_packages(&o.nixpkgs);
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

    for (rn, (olv, vs, ats)) in &known_versions {
        if let Some(lv) = olv {
          // Do not print outdated versions if there is use of most recet package
          if vs.contains(&lv as &str) { continue }
        }
        println!("repology {} {:?} | nixpkgs {:?} {:?}",
            rn, (*olv).clone().unwrap_or("<none>".to_string()), vs, ats);
    }

    missing_available.sort();
    missing_repology.sort();
    if o.verbose {
        eprintln!();
        eprintln!("Installed packages missing in available list: {:?}", missing_available);

        // TODO:
        // Should be relevant only when we fetch all repology data, not just '&outdated=1'
        //eprintln!();
        //eprintln!("Installed packages missing in repology output: {:?}", missing_repology);
    }
    Ok(())
}
