# minionrt CLI

Run containerized agents from the command line.

minionrt runs agents packaged as [OCI Container images](https://opencontainers.org/) (e.g. produced by [Docker](https://www.docker.com/)).
The running agents have no access to your host environment beyond what your container runtime (e.g. Docker) exposes by default.
minionrt is intended to be used on local git repositories that are proxied into the container via a git server binding to a container network interface.
This means only checked in code gets exposed to the agent, no tokens or secrets leaked, no agents reading confidential config files in your host environment.

While competing standards are still rapidly evolving, minionrt currently uses a [custom API](https://github.com/minionrt/spec) for communication between the container and the runtime on the host system.
However, there is already support for running agents supporting the [Agent Client Protocol (ACP)](https://agentclientprotocol.com).
For example, [Containerfile.codex](./Containerfile.codex) specifies a working container image for [OpenAI Codex](https://github.com/openai/codex) via [codex-acp](https://github.com/zed-industries/codex-acp).

## Quickstart

- Install a Rust toolchain using [rustup.rs](https://rustup.rs/).
- Clone the repository on your machine:
  ```console
  git clone https://github.com/minionrt/minionrt
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
  This will start the [default agent](Containerfile.codex) which is based on [codex-acp](https://github.com/zed-industries/codex-acp) and [codex](https://github.com/openai/codex) and provide it access to the git repository in the current directory.
  Note that it will only have access to content checked into git.
  Unstaged or ignored files (which may contain secrets) will deliberately **not** be accessible to the agent.
  Use `minion --help` and `minion run --help` for more information on CLI usage.

## License

This project is distributed under the terms of both the MIT license and the Apache License 2.0.
See [LICENSE-APACHE](LICENSE-APACHE) and [LICENSE-MIT](LICENSE-MIT) for details.
