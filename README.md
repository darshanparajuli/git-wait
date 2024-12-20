# git-wait

A simple wrapper utility around `git` that waits until `index.lock` file is no longer present before forwarding all the
args to `git` and running the command. This is especially useful when there are potentially other git commands running
on the same repo.

## Installation

0. Ensure Rust is [installed](https://rustup.rs/).
1. Run `cargo install git-wait`.

## Usage

```
git-wait <git args>
```

The set-it-and-forget-it approach:

```bash
# Put this in your shell config.
alias git=git-wait
```

Timeout can be set by setting `GIT_WAIT_TIMEOUT_MS` env var. It is in milliseconds.

```bash
# 5-second timeout:
$ GIT_WAIT_TIMEOUT_MS=5000 git-wait status
```

## Example

```bash
$ git-wait status
$ git-wait push
```

When `index.lock` is present:

```bash
$ git-wait status
Waiting on index.lock... done!
<regular git status output>
```