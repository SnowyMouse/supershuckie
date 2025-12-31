# Super Shuckie

TODO

## How to obtain

For Windows builds, refer to the [Releases] page.

[Releases]: https://github.com/SnowyMouse/supershuckie/releases

We do not currently provide pre-built binaries for Linux or macOS. Refer to the
build instructions if you are using either of these systems.

## Building

You will need Qt6, SDL3, Rust 1.89, CMake 4.0, Git, a C17 compiler, and a C++20
compiler.

### Linux (GNU/Linux)

On Linux, refer to your package manager for obtaining all needed software.

In many cases, you can also use Rustup to get Rust.

> **NOTE:** Some distros may not have all required software available through
> built-in repositories, or they may lack sufficient versions of some software.
> 
> If you have issues on your particular distro, don't be afraid to ask for help
> from the community, but be advised that we cannot guarantee support for all
> distros.

Run `build.sh` and locate the executables in the `build` directory.

### macOS

On macOS, you can satisfy these requirements by installing the following:
* Xcode Command Line Tools
* Homebrew (`brew install sdl3 qt@6`)
* Rustup

Run `build.sh` and locate the executables in the `build` directory.

### Windows

TODO
