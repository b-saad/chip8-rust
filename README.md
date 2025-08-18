# Chip-8

A Chip-8 interpreter written in Rust. Aims to accurately emulate a COSMAC-VIP. Compiles only for desktop platforms.

Note: while test roms all mostly pass, games don't seem to run, WIP

## Dependencies

### Fedora
- `alsa-lib-devel` for audio, `sudo dnf install alsa-lib-devel`

## Test roms
From https://github.com/Timendus/chip8-test-suite
- ALl roms rely on original behaviour for "ambiguous" instructions

## Usage
Note to faithfully emulate a COSMAC-VIP you must use all original behaviours i.e set all flags
```
Usage: chip8-rust [OPTIONS] --rom <ROM>

Options:
      --rom <ROM>                   Path to the Chip-8 ROM
      --cycle-rate <CYCLE_RATE>     How many CPU cycles should be completed per second [default: 700]
      --shift-instruction-original  Original behaviour of the shift instruction (default: false)
      --jump-with-offset-original   Original behaviour of jump with offset instruction (default: false)
      --store-and-load-original     Original behaviour of store and load instruction (default: false)
  -h, --help                        Print help
  -V, --version                     Print version
```

## Key Bindings

- Note: on the original COSMAC VIP, key presses are not recognized until the key is released. This behaviour was not replicated instead key presses are recognized as soon as they are actuated like a modern keyboard.

This input mapping is optimized for the left side of a QWERTY keyboard

```
Chip8 keypad         Keyboard mapping
1 | 2 | 3 | C        1 | 2 | 3 | 4
4 | 5 | 6 | D   =>   Q | W | E | R
7 | 8 | 9 | E   =>   A | S | D | F
A | 0 | B | F        Z | X | C | V
```

## Resources
- https://tobiasvl.github.io/blog/write-a-chip-8-emulator/
- https://github.com/Timendus/chip8-test-suite
