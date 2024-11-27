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

## Example

```bash
$ git-wait status
$ git-wait push
```