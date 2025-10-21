use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::time::{Duration,Instant};

use serde_derive::Deserialize;

use crate::cmd::*;
use crate::error::*;

/// Installed packages with available 'pname' and 'version' attributes.
#[derive(Eq, PartialEq, Ord, PartialOrd)]
pub(crate) struct Package {
    /// repology package name
    pub(crate) repology_name: String,

    /// nixpkgs 'pname' from available packages
    pub(crate) name: String,

    version: Option<String>,
    /// repology's characterization of the state: outdated, dev-only, etc.
    status: Option<String>,

    /// latest version available in some other repository
    /// Might not exist if latest version was added and then
    /// removed from repology.org.
    pub(crate) latest: Option<String>,
}

/// Returns list of all outdated derivations according to repology.
pub(crate) fn get_packages(
    cancel_fetch: &dyn Fn() -> bool,
) -> Result<BTreeSet<Package>, OldeError> {
    let mut r = BTreeSet::new();

    // We pull in all package ingo py paginating through
    //     https://repology.org/api/v1/projects/?inrepo=nix_unstable&outdated=1
    //     https://repology.org/api/v1/projects/${suffix}?inrepo=nix_unstable&outdated=1
    let mut suffix: String = "".to_string();

    // repology.org imposes a limit of 1 fetch per second:
    //     https://repology.org/api/v1
    // We keep here the time we are allowed to fetch next batch.
    let min_fetch_interval = Duration::from_secs(1);
    let mut next_fetch_time = Instant::now();

    loop {
        if cancel_fetch() {
            return Err(OldeError::Canceled(String::from("Repology fetch")));
        }

        // implement trivial throttling
        let now = Instant::now();
        if now.lt(&next_fetch_time)
        {
            // TODO: when encountered with a transient failure it might
            // be worthwile increasing the delay here and retry.
            // TODO: randomize the delay slightly to spread the delay
            // between multiple possible clients.
            let delay = next_fetch_time - now;
            log::debug!("Wait for {delay:?} before next fetch");
            std::thread::sleep(delay);
        }

        let url =
            format!("https://repology.org/api/v1/projects/{suffix}?inrepo=nix_unstable&outdated=1");
        // TODO: add an optional user identity string.
        let user_agent = format!(
            "{}/{} (+{})",
            env!("CARGO_PKG_NAME"),
            env!("CARGO_PKG_VERSION"),
            "https://github.com/trofi/nix-olde"
        );

        log::debug!("Fetching from repology: {:?}", suffix);
        let contents_u8 = run_cmd(&[
            "curl",
            // Don't write to stderr things that are not problems.
            "--no-progress-meter",
            // Fail `curl` command when server returns errors like
            // throttling (429).
            "--fail-with-body",
            // json is good to compress: usually a 3x improovement
            // on large responses. It also happens to fetch faster
            // on slow connections.
            "--compressed",
            "--user-agent",
            &user_agent,
            &url,
        ])?;

        // Make sure we allow at least min_fetch_interval between previous
        // `curl` finish and next `curl` start.
        next_fetch_time = Instant::now() + min_fetch_interval;

        // {
        //   "python:networkx": [
        //     {
        //       "repo": "nix_unstable",
        //       "visiblenamename": "python3.10-networkx",
        //       "version": "2.8.6",
        //       "status": "outdated",
        //     },

        #[derive(Deserialize, Debug)]
        /// Dervivation description with subset of fields needed to detect outdated packages.
        struct Repology {
            repo: String,
            visiblename: Option<String>,
            version: Option<String>,
            status: Option<String>,
        }

        let pkgs: BTreeMap<String, Vec<Repology>> = serde_json::from_slice(contents_u8.as_slice())?;

        let mut next_suffix = suffix.clone();
        for (n, vs) in &pkgs {
            next_suffix = n.clone() + "/";

            let olatest_entry = vs.iter().find_map(|e| {
                if e.status == Some("newest".to_string()) || e.status == Some("unique".to_string())
                {
                    Some(e)
                } else {
                    None
                }
            });
            let latest = match olatest_entry {
                None => None,
                Some(oe) => oe.version.clone(),
            };

            // There can be multiple nix_unstable package entries for a
            // single repology entry: pycropto vs pycryptodome.
            // Store all of them.
            for v in vs {
                if v.repo != "nix_unstable" {
                    continue;
                }

                match &v.visiblename {
                    None => {
                        eprintln!("Skipping an entry without 'name' attribyte: {v:?}");
                        log::debug!(
                            "JSON for entry: {:?}",
                            String::from_utf8(contents_u8.clone())
                        );
                        continue;
                    }
                    Some(vn) => {
                        r.insert(Package {
                            repology_name: n.clone(),
                            name: vn.clone(),
                            version: v.version.clone(),
                            status: v.status.clone(),
                            latest: latest.clone(),
                        });
                    }
                }
            }
        }
        if suffix == next_suffix {
            break;
        }
        suffix = next_suffix;
    }

    Ok(r)
}
