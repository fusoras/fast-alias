# fa fish completion (static — installed once, no `fa completions` subcommand).
# Only deps: the `fa` binary on PATH + sed/grep/tr/sort.
# Caveat: `fa list` hides non-scaffold recipes (no create/files/steps), so those
# never appear as candidates. Reinstall this file if the CLI surface changes.

function __fa_strip_ansi
    sed -e 's/\x1b\[[0-9;]*m//g'
end

function __fa_recipes -d 'Scaffold recipe names from fa list'
    command -v fa >/dev/null 2>&1; or return 0
    fa list 2>/dev/null \
        | __fa_strip_ansi \
        | sed -n '/^Recipes:/,/^Aliases:/p' \
        | grep -v '^Recipes:' \
        | grep -v '^Aliases:' \
        | grep -v 'Usage:' \
        | sed -e 's/·.*//' -e 's/^ *//' -e 's/,/ /g' \
        | tr -s ' ' '\n' \
        | grep -v '^$' \
        | sort -u
end

function __fa_alias_names -d 'Alias command names from fa list'
    command -v fa >/dev/null 2>&1; or return 0
    fa list 2>/dev/null \
        | __fa_strip_ansi \
        | sed -n '/^Aliases:/,$p' \
        | grep '^    [^ ]' \
        | grep -v ':$' \
        | sed -e 's/·.*//' -e 's/^ *//' -e 's/,/ /g' \
        | tr -s ' ' '\n' \
        | grep -v '^$' \
        | sort -u
end

# Top level: subcommands (+ short forms) and globals.
complete -c fa -f -n __fish_use_subcommand -a new -d 'Scaffold a project from a recipe'
complete -c fa -f -n __fish_use_subcommand -a list -d 'List recipes and aliases'
complete -c fa -f -n __fish_use_subcommand -a search -d 'Search recipes and aliases'
complete -c fa -f -n __fish_use_subcommand -a show -d 'Show recipe or alias details'
complete -c fa -f -n __fish_use_subcommand -a alias -d 'Run a catalog command'
complete -c fa -f -n __fish_use_subcommand -a self-update -d 'Update fa in-place'
complete -c fa -f -n __fish_use_subcommand -a self-uninstall -d 'Remove fa, state and config'
complete -c fa -f -n __fish_use_subcommand -s n -d 'Shorthand for new'
complete -c fa -f -n __fish_use_subcommand -s l -d 'Shorthand for list'
complete -c fa -f -n __fish_use_subcommand -s a -d 'Shorthand for alias'
complete -c fa -f -n __fish_use_subcommand -s v -l version -d 'Print version'
complete -c fa -f -n __fish_use_subcommand -s h -l help -d 'Print help'

# new: recipes first, flags always.
complete -c fa -f -n '__fish_seen_subcommand_from new; and test (count (commandline -opc)) -eq 2' -a '(__fa_recipes)'
complete -c fa -f -n '__fish_seen_subcommand_from new' -s v -l variant -d 'Toolchain variant' -r
complete -c fa -f -n '__fish_seen_subcommand_from new' -s d -l dry-run -d 'Preview without changes'
complete -c fa -f -n '__fish_seen_subcommand_from new' -l no-install -d 'Skip dependency installation'

# list / self-update / self-uninstall flags.
complete -c fa -f -n '__fish_seen_subcommand_from list' -s s -l show-hidden -d 'Show hidden recipes'
complete -c fa -f -n '__fish_seen_subcommand_from self-update' -s d -l dry-run -d 'Preview without modifying binary'
complete -c fa -f -n '__fish_seen_subcommand_from self-uninstall' -s y -l yes -d 'Confirm removal of config and state'
complete -c fa -f -n '__fish_seen_subcommand_from self-uninstall' -s n -l no -d 'Keep config and state'
complete -c fa -f -n '__fish_seen_subcommand_from self-uninstall' -s d -l dry-run -d 'Preview without deleting files'

# show / search: recipes + aliases. alias: alias names.
complete -c fa -f -n '__fish_seen_subcommand_from show search; and test (count (commandline -opc)) -eq 2' -a '(__fa_recipes) (__fa_alias_names)'
complete -c fa -f -n '__fish_seen_subcommand_from alias; and test (count (commandline -opc)) -eq 2' -a '(__fa_alias_names)'
