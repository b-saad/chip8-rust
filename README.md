# Chip-8

A Chip-8 compiler written in Rust. Compiles only for desktop platforms.

## Dependencies

### Fedora
- `alsa-lib-devel` for audio, `sudo dnf install alsa-lib-devel`

## Usage
```
A Chip-8 Emulator

Usage: chip8-rust [OPTIONS] --rom <ROM>

Options:
      --rom <ROM>                   Path to the Chip-8 ROM
      --shift-instruction-original  Original behaviour of the shift instruction (default: true)
      --jump-with-offset-original   Original behaviour of jump with offset instruction (default: true)
      --store-and-load-original     Original behaviour of store and load instruction (default: false)
  -h, --help                        Print help
  -V, --version                     Print version

```

## Key Bindings

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
