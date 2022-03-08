# mpw-emu

An extremely ridiculous weekend project that tries to do user-mode emulation of PowerPC executables for classic Mac OS, to run the CodeWarrior C++ compiler without faffing about with SheepShaver or QEMU.

## Features

- Speaks MacBinary so you can interact with Mac files on Windows
- Implements enough nonsense to compile object files using MWCPPC from CodeWarrior Pro 1 *and* decompile resources using DeRez!
- Probably won't destroy your file system
- It's written in Rust! 🦀

## TODO

- Implement the missing relocations for the PEF linker
- Implement more of the C standard library
  - Figure out a way to make printf better (maybe fork one of the existing Rust implementations)
- Implement more of the Macintosh Toolbox(tm)
- Add MacBinary writing so you can save files on Windows
  - Maybe support AppleDouble as well?
- Do something more elegant for CR-LF conversion
- Test whether `#include`ing files works
- Get more MPW executables working
  - Investigate why some of them aren't PEF files (are these XCOFF?)
