# mpw-emu

An extremely ridiculous weekend project that tries to do user-mode emulation of PowerPC executables for classic Mac OS, to run the CodeWarrior C++ compiler without faffing about with SheepShaver or QEMU.

## Features

- Loads PEF executables wrapped in MacBinary format
- Implements enough nonsense to compile object files using MWCPPC from CodeWarrior Pro 1
- Probably won't destroy your file system
- It's written in Rust! 🦀

## TODO

- Implement the missing relocations for the PEF linker
- Implement more of the C standard library
- Implement more of the Macintosh Toolbox(tm)
- Make the file system bridge more robust
  - Does my half-assed code to bridge into the classic Mac file system API work on a Unix host?
  - Use file extensions to pick a type/creator code rather than assuming `TEXT` (this would probably make MWDumpPPC work!)
- Check if I can make loading files more robust
  - Should I try to support packed formats other than MacBinary?
  - Using `file/..namedfork/rsrc` might let me load raw files on MacOS
- Test whether `#include`ing files works
- Get more MPW executables working
  - Investigate why some of them aren't PEF files (are these XCOFF?)
