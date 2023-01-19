`nix-olde` is a tool to show details about outdated packages in your
`NixOS` system using <https://repology.org/> database.

# Usage example

The simplest way to use it is to just run it:

```
$ cargo build --release && target/release/nix-olde

Fetching from repology: ""
Fetching from repology: "bitlbee-discord/"
packages: installed done!
...
Fetching from repology: "zxing-cpp-nu-book/"
packages: repology done
...
a52dec is outdated: repology_latest="0.8.0" nixpkgs_available={"0.7.4"} (nixpkgs_attributes={"nixos.a52dec"})
afdko is outdated: repology_latest="3.9.2" nixpkgs_available={"3.9.0"} (nixpkgs_attributes={"nixos.python310Packages.afdko"})
alsa-lib is outdated: repology_latest="1.2.8" nixpkgs_available={"1.2.7.2"} (nixpkgs_attributes={"nixos.alsa-lib"})
alsa-ucm-conf is outdated: repology_latest="1.2.8" nixpkgs_available={"1.2.7.1"} (nixpkgs_attributes={"nixos.alsa-ucm-conf"})
appstream is outdated: repology_latest="0.15.6" nixpkgs_available={"0.15.5"} (nixpkgs_attributes={"nixos.appstream"})
atomicparsley is outdated: repology_latest="20221229.172126.d813aa6" nixpkgs_available={"20210715.151551.e7ad03a"} (nixpkgs_attributes={"nixos.atomicparsley"})
audit is outdated: repology_latest="3.0.9" nixpkgs_available={"2.8.5"} (nixpkgs_attributes={"nixos.audit"})
autogen is outdated: repology_latest="5.19.96" nixpkgs_available={"5.18.16"} (nixpkgs_attributes={"nixos.autogen"})
...
```

Here output consists of 2 parts:

1. log lines around fetching data from various sources
2. dump of a list of outdated packages

# Other options

There are a few:

```
$ ./mkrun.bash --help

Usage: nix-olde [OPTIONS]

Options:
  -n, --nixpkgs <NIXPKGS>  Alternative path to <nixpkgs> location
  -v, --verbose            Enable extra verbosity to report unexpected events, fetch progress and so on
  -h, --help               Print help information
  -V, --version            Print version information
```

Of all the above the most interesting one is probably `--nixpkgs`.
Using it you can check how fresh your system would be it it was built
out of, say, `staging` branch:

```
$ ./mkrun.bash --nixpkgs https://github.com/NixOS/nixpkgs/archive/refs/heads/staging.tar.gz
Fetching from repology: ""
Fetching from repology: "bip/"
...
xterm is outdated: repology_latest="378" nixpkgs_available={"377"} (nixpkgs_attributes={"xterm"})
xz is outdated: repology_latest="5.4.1" nixpkgs_available={"5.4.0"} (nixpkgs_attributes={"xz"})
zxing-cpp-nu-book is outdated: repology_latest="2.0.0" nixpkgs_available={"1.4.0"} (nixpkgs_attributes={"zxing-cpp"})
```

# How it works

The theory is simple: fetch data from various data sources and join
them together. Each data source returns a tuple of package name,
package version and possibly a bit of metadata around it (name in other
systems, possible status).

Currently used data sources are:

- installed packages: uses `nix-instantiate` / `nix
  show-derivation`. Provides fields:
  * `name` (example: `python3.10-networkx-2.8.6`)
  * `version` (example: `2.8.6`)
- available packages: uses `nix-env -qa --json` tool, memory
  hungry. Provides fields:
  * [keyed from installed packages] `name` (example: `python3.10-networkx-2.8.6`)
  * `attribute`: `nixpkgs` attribute path (example: `nixos.python310Packages.networkx`)
  * `pname`: `name` with `version` component dropped (example: `nixos.python310Packages.networkx`)
  * `version` (example: `2.8.6`)
- <https://repology.org/> `json` database: uses
  `https://repology.org/api/v1/projects/` `HTTP` endpoint. Provides
  fields:
  * `repo`: package repository (example: "nix_unstable")
  * [keyed from available] `name`: with `version` component
    dropped (example: `nixos.python310Packages.networkx`).
  * `version` (example: `2.8.6`)
  * `status`: package status in repository (examples: "newest",
    "outdared").

