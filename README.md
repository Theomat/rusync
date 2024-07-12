# rusync

## Usage

```bash
rusync new toto
rusync add toto ./my_local_script.sh remote:./some_folder/remote_script.sh
rusync
```

``rusync`` will synchronise all syncs shown by ``rusync ls``, which list all syncs that have a file in a folder or in a subfolder of the current working directory.

## Install

```bash
cargo install --path .
```

### Auto Completion

Here is an example if you are using zsh:

```bash
rusync completions zsh > ~/.zfunc/_rusync 
```

Then start a new terminal.
