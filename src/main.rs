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


/// Installed packages with available 'pname' and 'version' attributes.
#[derive(Eq, PartialEq, Ord, PartialOrd)]
struct LocalInstalledPackage {
    /// Full not-quite-'pname' + 'version' from package environment.
    name: String,
    /// 'version' attribute from package environment. Most trusted.
    version: String,
}

/// Returns list of all used derivations in parsed form.
// TODO: add parameters like a directory defining nixpkgs path, a system expression.
fn get_local_installed_packages() -> BTreeSet<LocalInstalledPackage> {
    // TODO: is there a 'nix' command equivalent?
    let out_u8 = run_cmd(&["nix-instantiate",
                           "<nixpkgs/nixos>",
                           "-A", "system"
                           ]);
    // Returns path to derivation file (and a newline)
    let out_s = String::from_utf8(out_u8).expect("utf8");
    // Have to drop trailing newline.
    let drv_path = out_s.trim();

    // TODO: pass in experimental command flags to make it work on
    // default `nix` installs as well. 
    let drvs_u8 = run_cmd(&["nix", "show-derivation", "-r", drv_path]);
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
    /// Dervivation description with subset of fields needed to detect stale packages.
    struct Installed { env: DrvEnv, }

    let drvs: BTreeMap<String, Installed> =
        serde_json::from_slice(drvs_u8.as_slice()).expect("valid json");

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
    // Actual command is taken from pkgs/top-level/make-tarball.nix for
    // 'packages.json.br' build. It's used by repology as is.
    let ps_u8 = run_cmd(&["nix-env",
                           "-qa",
                           "--json",
                           "--arg", "config", "import <nixpkgs/pkgs/top-level/packages-config.nix>",
                           "--option", "build-users-group", "\"\"",
                           ]);
    // "nixos.python310Packages.networkx": {
    //   "name": "python3.10-networkx-2.8.6",
    //   "pname": "python3.10-networkx",
    //   "version": "2.8.6"
    // },

    #[derive(Deserialize)]
    struct Available { name: String, pname: String, version: String }

    let ps: BTreeMap<String, Available> =
        serde_json::from_slice(ps_u8.as_slice()).expect("valid json");

    ps.iter().filter_map(|(attr, a)|
        Some(LocalAvailablePackage{
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

/// Returns list of all stale derivations according to repology.
fn get_repology_packages() -> BTreeSet<RepologyPackage> {
    let mut r = BTreeSet::new();

    // We pull in all package ingo py paginating through
    //     https://repology.org/api/v1/projects/?inrepo=nix_unstable&outdated=1
    //     https://repology.org/api/v1/projects/${suffix}?inrepo=nix_unstable&outdated=1
    let mut suffix: String = "".to_string();

    loop {
        let url = format!("https://repology.org/api/v1/projects/{suffix}?inrepo=nix_unstable&outdated=1");

        println!("Fetching from repology: {:?}", suffix);
        let contents_u8 = run_cmd(&["curl", "--compressed", "-s", &url]);
        // {
        //   "python:networkx": [
        //     {
        //       "repo": "nix_unstable",
        //       "name": "python3.10-networkx",
        //       "version": "2.8.6",
        //       "status": "outdated",
        //     },

        #[derive(Deserialize, Debug)]
        /// Dervivation description with subset of fields needed to detect stale packages.
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
