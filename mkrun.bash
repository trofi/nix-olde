#!/usr/bin/env bash

cargo build --release -q && target/release/nix-olde "$@"
