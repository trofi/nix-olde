use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::process::Command;
use std::cmp::min;

use serde_derive::Deserialize;

fn u8_to_s(a: &[u8], nbytes: usize) -> String {
  let prefix = &a[..min(nbytes, a.len())];
  String::from_utf8(prefix.to_vec()).expect("valid utf8")
}

/// Runs 'cmd' and returns stdout.
/// TODO: add error handling.
fn run_cmd(args: &[&str]) -> Vec<u8> {
    let cmd = Command::new(args[0]).args(&args[1..])
                      .output()
                      .expect("Failed to run command");
    // TODO: error handling
    if !cmd.status.success() {
      println!("Command {:?} failed with status: {:?}", args, cmd.status);
      println!("stdout: {:?}", u8_to_s(&cmd.stdout, 40));
      println!("stderr: {:?}", u8_to_s(&cmd.stderr, 40));
    }

    assert!(cmd.status.success());

    cmd.stdout
}

#[derive(Deserialize)]
struct DrvEnv {
    /// Full not-quite-'pname' + 'version' from package environment.
    name: Option<String>,
    /// 'version' attribute from package environment. Most trusted.
    version: Option<String>,
}

#[derive(Deserialize)]
/// Dervivation description with subset of fields needed to detect stale packages.
struct Drv { env: DrvEnv, }

/// Installed packages with available 'pname' and 'version' attributes.
#[derive(Eq, PartialEq, Ord, PartialOrd)]
struct LocalInstalledPackage {
    name: String,
    version: String,
}

/// Returns list of all used derivations in parsed form.
// TODO: add parameters like a directory defining nixpkgs path, a system expression.
// TODO: add handling for packages with 'name' attribute and without 'pname' / 'version'.
fn get_local_installed_packages() -> BTreeSet<LocalInstalledPackage> {
    // Run an equivalent of:
    // $ nix show-derivation -r $(nix-instantiate '<nixpkgs/nixos>' -A system)
    // {
    //   "/nix/store/...-python3.10-networkx-2.8.6.drv": {
    //     "args": [
    //       "-e",
    //       "/nix/store/...-default-builder.sh"
    //     ],
    //     "builder": "/nix/store/...-bash-5.1-p16/bin/bash",
    //     "env": {
    //       "builder": "/nix/store/...-bash-5.1-p16/bin/bash",
    //       "name": "python3.10-networkx-2.8.6",
    //       "pname": "networkx",
    //       "version": "2.8.6"
    //       ...

    // TODO: is there a 'nix' command equivalent?
    let out_u8 = run_cmd(&["nix-instantiate",
                           "<nixpkgs/nixos>",
                           "-A", "system"
                           ]);

    let out_s = String::from_utf8(out_u8).expect("utf8");
    // Have to drop trailing newline.
    let drv_path = out_s.trim();

    // TODO: pass in experimental command flags
    let drvs_u8 = run_cmd(&["nix", "show-derivation", "-r", drv_path]);
    let drvs: BTreeMap<String, Drv> = serde_json::from_slice(drvs_u8.as_slice()).expect("valid json");

    drvs.iter().filter_map(|(_drv, oenv)|
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
    ).collect()
}

#[derive(Deserialize)]
/// Dervivation description with subset of fields needed to detect stale packages.
struct Available {
    name: String,
    pname: String,
    version: String,
}

/// Installed packages with available 'pname' and 'version' attributes.
#[derive(Eq, PartialEq, Ord, PartialOrd)]
struct LocalAvailablePackage {
    attribute: String,
    name: String,
    pname: String,
    version: String,
}

/// Returns list of all available packages in parsed form.
// TODO: add parameters like a directory defining nixpkgs path, a system expression.
fn get_local_available_packages() -> BTreeSet<LocalAvailablePackage> {
    // Run an equivalent of:
    // $ nix-env -qa --json [TODO maybe --meta?]
    // "nixos.python310Packages.networkx": {
    //   "name": "python3.10-networkx-2.8.6",
    //   "outputName": "out",
    //   "outputs": {
    //     "dist": null,
    //     "out": null
    //   },
    //   "pname": "python3.10-networkx",
    //   "system": "x86_64-linux",
    //   "version": "2.8.6"
    // },
    //
    // Actual command is taken from pkgs/top-level/make-tarball.nix for
    // 'packages.json.br' build. It's used by repology as is.

    let ps_u8 = run_cmd(&["nix-env",
                           "-qa",
                           "--json",
                           "--arg", "config", "import <nixpkgs/pkgs/top-level/packages-config.nix>",
                           "--option", "build-users-group", "\"\"",
                           ]);

    let ps: BTreeMap<String, Available> = serde_json::from_slice(ps_u8.as_slice()).expect("valid json");

    ps.iter().filter_map(|(attr, a)|
        Some(LocalAvailablePackage{
            // TODO: remove clones and pass ownership from 'ps'
            attribute: attr.clone(),
            name: a.name.clone(),
            pname: a.pname.clone(),
            version: a.version.clone(),
        })
    ).collect()
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

#[derive(Deserialize, Debug)]
/// Dervivation description with subset of fields needed to detect stale packages.
struct RepologyEntry {
    repo: String,

    // TODO: what about 'srcname' and 'visiblename'?
    name: Option<String>,

    // TODO: also has 'origversion'
    version: String,

    // "outdated" / "newest"
    status: String,
}

/// Returns list of all stale derivations according to repology.
/// TODO: fetch data from repology site
fn get_repology_packages() -> BTreeSet<RepologyPackage> {
    // Parse repology json entries in form of:
    //     {
    //       "python:networkx": [
    //         {
    //         {
    //           "repo": "nix_unstable",
    //           "name": "python3.10-networkx",
    //           "visiblename": "python3.10-networkx",
    //           "version": "2.8.6",
    //           "maintainers": [
    //             "fallback-mnt-nix@repology"
    //           ],
    //           "licenses": [
    //             "BSD-3-Clause"
    //           ],
    //           "summary": "Library for the creation, manipulation, and study of the structure, dynamics, and functions of complex networks",
    //           "categories": [
    //             "python310Packages"
    //           ],
    //           "status": "outdated",
    //           "origversion": null
    //         },


    let mut r = BTreeSet::new();

    // We pull in all package ingo py paginating through
    //     https://repology.org/api/v1/projects/?inrepo=nix_unstable
    //     https://repology.org/api/v1/projects/${suffix}?inrepo=nix_unstable
    // We stop iteration when we can't advance suffix.
    //
    // Note: we fetch not just outdated, but all the package versions
    // known to repology. For some use cases we could fetch only
    // outdated:
    //     https://repology.org/api/v1/projects/?inrepo=nix_unstable&outdated=1"
    //     https://repology.org/api/v1/projects/${suffix}?inrepo=nix_unstable&outdated=1"
    let mut suffix: String = "".to_string();

    loop {
        //let url = format!("https://repology.org/api/v1/projects/{suffix}?inrepo=nix_unstable");
        let url = format!("https://repology.org/api/v1/projects/{suffix}?inrepo=nix_unstable&outdated=1");

        println!("Fetching from repology: {:?}", suffix);
        let contents_u8 = run_cmd(&["curl", "--compressed", "-s", &url]);

        let pkgs: BTreeMap<String, Vec<RepologyEntry>> = serde_json::from_slice(contents_u8.as_slice()).expect("valid json");

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

    r
}

fn main() {
    // TODO: add basic help and commandline

    let (repology_ps, installed_ps, available_ps) = (|| {
       let mut r = BTreeSet::<RepologyPackage>::new();
       let mut i = BTreeSet::<LocalInstalledPackage>::new();
       let mut a = BTreeSet::<LocalAvailablePackage>::new();

       // Each of threads is somewhat slow to proceed:
       // - Repology thread is network-bound
       // - Installed and available threads are CPU-bound
       std::thread::scope(|s| {
         s.spawn(|| { r = get_repology_packages(); println!("packages: repology done"); });
         s.spawn(|| { i = get_local_installed_packages(); println!("packages: installed done!"); });
         s.spawn(|| { a = get_local_available_packages(); println!("packages: available done!"); });
       });

       (r, i, a)
    }) ();

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
        println!("{} is outdated: repology_latest={:?} nixpkgs_available={:?} (nixpkgs_attributes={:?})", rn, (*olv).clone().unwrap_or("<none>".to_string()), vs, ats);
    }

    missing_available.sort();
    missing_repology.sort();
    println!();
    println!("Installed packages missing in available list: {:?}", missing_available);

    // TODO:
    // Should be relevant only when we fetch all repology data, not just '&outdated=1'
    //println!();
    //println!("Installed packages missing in repology output: {:?}", missing_repology);
}
