# autocrap

a cross-platform userspace driver for the Novation Nocturn, and hopefully other Automap-based controllers in the future!

## features

- input and output via MIDI or OSC
- highly configurable control mappings via JSON files

## compatibility

- macOS (tested with 10.14)
- Linux (tested with Debian 12)
- Windows? (not tested yet)
- possibly other platforms supported by Rust and libusb

## installation

binaries coming soon! in the meantime, see [#building](building).

## usage

TODO

## building

you will need:

- rustc (tested with 1.79.0)
- Cargo

```shell
cd autocrap
cargo build --release
```

this creates an executable under `target/release` called `autocrap`, which can be moved wherever you like.

## disclaimer

all trademarks are property of their respective owners. all company and product names used in this repository are for identification purposes only. use of these names, trademarks and brands does not imply endorsement.
