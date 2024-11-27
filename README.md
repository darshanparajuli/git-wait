# git-wait

A simple wrapper utility around `git` that waits until `index.lock` file is no longer present before forward all the
args to `git` and running the command.

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