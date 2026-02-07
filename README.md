# minionrt CLI

Run compatible agents from the command line.

> [!WARNING]
> This project is in an early state of development.
> Expect bugs and missing features.

## Quickstart

- Install a Rust toolchain using [rustup.rs](https://rustup.rs/).
- Clone the repository on your machine:
  ```console
  git clone --recurse-submodules https://github.com/minionrt/minionrt
  ```
- To locally install the `minion` executable, run:
  ```console
  cargo install --path cli
  ```
  The binary `minion` gets installed to `~/.cargo/bin/minion`; make sure that this directory is in your `$PATH`.
- Login to one of the supported LLM providers:
  ```console
  minion login --help
  ```
- Navigate to any git repository cloned on your local machine and run:
  ```console
  minion run
  ```
  This will start the [default agent](https://github.com/minionrt/default-minion) and provide it access to the git repository in the current directory.
  Note that it will only have access to content checked into git.
  Unstaged or ignored files (which may contain secrets) will deliberately **not** be accessible to the agent.
  Use `minion --help` and `minion run --help` for more information on CLI usage.

## License

This project is distributed under the terms of both the MIT license and the Apache License 2.0.
See [LICENSE-APACHE](LICENSE-APACHE) and [LICENSE-MIT](LICENSE-MIT) for details.
