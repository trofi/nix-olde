`nix-olde` is a tool to show details about outdated packages in your
`NixOS` system using <https://repology.org/> database.

It can use both default `<nixpkgs>` or custom URL or path specified via
`--nixpkgs / -n` parameter.

I usually use the tool to see what I could update in `nixpkgs` that my
system currently uses.

# Dependencies

At runtime `nix-olde` uses 2 external packages and expects them in `PATH`:

- `curl` to fetch `repology.org` reports
- `nix` to query locally installed and available packages

To build `nix-olde` you will need `rustc` and `cargo`. `Cargo.tml`
contains more detailed description of dependencies.

# Running it

There is no `Cargo.lock` package or uploaded crate just yet. I'm not
sure if such a small tool warrants it. Thus you need to clone and build
it using `cargo` build tool:

```
$ git clone https://github.com/trofi/nix-olde.git
$ cd nix-olde
$ cargo build
```

Now you should be ready to run it.

Diff against current system:

```
$ ./target/debug/nix-olde
```

Diff against latest `staging`:

```
$ ./target/debug/nix-olde -n https://github.com/NixOS/nixpkgs/archive/refs/heads/staging.tar.gz
```

Or against local checkout:

```
$ ./target/debug/nix-olde -n ./nixpkgs
```

I usually use small shell wrapper to build-and-run against local
`staging` checkout:

```
$ ./mkrun.bash -n $(realpath ~/n)
```

# Typical output

```
$ cargo build && target/debug/nix-olde
...
Fetching 'repology'
Fetching 'available'
Fetching 'installed'
'installed' done, took 6.10 s.
'available' done, took 12.22 s.
'repology' done, took 75.38 s.

repology a52dec "0.8.0" | nixpkgs {"0.7.4"} {"nixos.a52dec"}
repology alsa-lib "1.2.8" | nixpkgs {"1.2.7.2"} {"nixos.alsa-lib"}
repology alsa-ucm-conf "1.2.8" | nixpkgs {"1.2.7.1"} {"nixos.alsa-ucm-conf"}
repology appstream "0.15.6" | nixpkgs {"0.15.5"} {"nixos.appstream"}
repology atomicparsley "20221229" | nixpkgs {"20210715.151551.e7ad03a"} {"nixos.atomicparsley"}
repology audit "3.0.9" | nixpkgs {"2.8.5"} {"nixos.audit"}
repology autogen "5.19.96" | nixpkgs {"5.18.16"} {"nixos.autogen"}
...
repology xrandr "1.5.2" | nixpkgs {"1.5.1"} {"nixos.xorg.xrandr"}
repology xset "1.2.5" | nixpkgs {"1.2.4"} {"nixos.xorg.xset"}
repology xsetroot "1.1.3" | nixpkgs {"1.1.2"} {"nixos.xorg.xsetroot"}
repology xterm "378" | nixpkgs {"377"} {"nixos.xterm"}
repology xz "5.4.1" | nixpkgs {"5.4.0"} {"nixos.xz"}
repology zxing-cpp-nu-book "2.0.0" | nixpkgs {"1.4.0"} {"nixos.zxing-cpp"}

388 of 1518 (25.56%) installed packages are outdated according to https://repology.org.

Some installed packages are missing in available list: 68
  Add '--verbose' to get it's full list.
```

# Other options

There are a few options:

```
$ ./mkrun.bash --help

A tool to show outdated packages in current system according to repology.org database

Usage: nix-olde [OPTIONS]

Options:
  -n, --nixpkgs <NIXPKGS>  Alternative path to <nixpkgs> location
  -v, --verbose            Enable extra verbosity to report unexpected events, fetch progress and so on
  -h, --help               Print help
  -V, --version            Print version
```

`--nixpkgs` / `-n` is most useful when you are looking for packages that
were not yet updated in a particular development branch of `nixpkgs`
repository (usually `staging` or `master`).

# How `nix-olde` works

The theory is simple: fetch data from various data sources and join
them together. Each data source returns a tuple of package name,
package version and possibly a bit of metadata around it (name in other
systems, possible status).

Currently used data sources are:

- installed packages: uses `nix-instantiate` / `nix show-derivation`.
  Provides fields:
  * `name` (example: `python3.10-networkx-2.8.6`)
  * `version` (example: `2.8.6`)
- available packages: uses `nix-env -qa --json` tool, memory hungry.
  Provides fields:
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

# License

`nix-olde` is distributed under
[MIT license](https://opensource.org/licenses/MIT).
