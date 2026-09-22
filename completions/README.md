# Shell completions for `fa` (static scripts)

Tab-completion for subcommands, flags, recipe names (`new`/`show`) and alias
names (`alias`/`show`). Install once per shell; no `fa` subcommand needed.

Recipes/aliases are read live from `fa list` (ANSI-stripped, parsed with
sed/grep/tr/sort). Only scaffold recipes appear — `fa list` hides recipes
without `create`/`files`/`steps`.

## Install

- Bash (Debian): `sudo cp completions/fa.bash /usr/share/bash-completion/completions/fa`
- Bash (Termux): `cp completions/fa.bash "$PREFIX/share/bash-completion/completions/fa"`
- Zsh (Debian): `sudo cp completions/fa.zsh /usr/share/zsh/site-functions/_fa`
- Zsh (Termux): `cp completions/fa.zsh "$PREFIX/share/zsh/site-functions/_fa"`
- Fish (Debian/Termux): `cp completions/fa.fish ~/.config/fish/completions/fa.fish`

Restart the shell (or `source` the file) after installing.

## Maintenance

Static scripts: when the CLI surface changes (new subcommand/flag), update
`fa.bash`, `fa.zsh` and `fa.fish` by hand — there is no generator.
