# fa bash completion (static — installed once, no `fa completions` subcommand).
# Only deps: the `fa` binary on PATH + sed/grep/tr/sort. No ANSI codes assumed:
# output is stripped before parsing, so colored/plain `fa list` both work.
# Caveat: `fa list` hides non-scaffold recipes (no create/files/steps), so those
# never appear as candidates. Reinstall this file if the CLI surface changes.

# Print scaffold recipe names (one per line), canonical + aliases.
_fa_recipes() {
    command -v fa >/dev/null 2>&1 || return 0
    fa list 2>/dev/null \
        | sed -e 's/\x1b\[[0-9;]*m//g' \
        | sed -n '/^Recipes:/,/^Aliases:/p' \
        | grep -v '^Recipes:' \
        | grep -v '^Aliases:' \
        | grep -v 'Usage:' \
        | sed -e 's/·.*//' -e 's/^ *//' -e 's/,/ /g' \
        | tr -s ' ' '\n' \
        | grep -v '^$' \
        | sort -u
}

# Print alias command names (one per line), canonical + aliases.
_fa_alias_names() {
    command -v fa >/dev/null 2>&1 || return 0
    fa list 2>/dev/null \
        | sed -e 's/\x1b\[[0-9;]*m//g' \
        | sed -n '/^Aliases:/,$p' \
        | grep '^    [^ ]' \
        | grep -v ':$' \
        | sed -e 's/·.*//' -e 's/^ *//' -e 's/,/ /g' \
        | tr -s ' ' '\n' \
        | grep -v '^$' \
        | sort -u
}

_fa_complete() {
    local cur prev sub
    COMPREPLY=()
    cur="${COMP_WORDS[COMP_CWORD]}"
    prev="${COMP_WORDS[COMP_CWORD-1]}"
    sub="${COMP_WORDS[1]}"

    # First arg: subcommands (+ short forms + direct alias names).
    if [ "$COMP_CWORD" -eq 1 ]; then
        COMPREPLY=( $(compgen -W "new list search show alias self-update self-uninstall -n -l -a --help --version -h -v $(_fa_alias_names)" -- "$cur") )
        return 0
    fi

    case "$sub" in
        new|-n)
            case "$prev" in
                -v|--variant) return 0 ;; # variant value: no completion
            esac
            if [ "${cur#-}" != "$cur" ]; then
                COMPREPLY=( $(compgen -W "-v --variant -d --dry-run --no-install --help -h" -- "$cur") )
            elif [ "$COMP_CWORD" -eq 2 ]; then
                COMPREPLY=( $(compgen -W "$(_fa_recipes)" -- "$cur") )
            fi
            return 0
            ;;
        list|-l)
            COMPREPLY=( $(compgen -W "-s --show-hidden --help -h" -- "$cur") )
            return 0
            ;;
        search)
            if [ "${cur#-}" != "$cur" ]; then
                COMPREPLY=( $(compgen -W "--help -h" -- "$cur") )
            elif [ "$COMP_CWORD" -eq 2 ]; then
                COMPREPLY=( $(compgen -W "$(_fa_recipes) $(_fa_alias_names)" -- "$cur") )
            fi
            return 0
            ;;
        show)
            if [ "${cur#-}" != "$cur" ]; then
                COMPREPLY=( $(compgen -W "--help -h" -- "$cur") )
            elif [ "$COMP_CWORD" -eq 2 ]; then
                COMPREPLY=( $(compgen -W "$(_fa_recipes) $(_fa_alias_names)" -- "$cur") )
            fi
            return 0
            ;;
        alias|-a)
            if [ "${cur#-}" != "$cur" ]; then
                COMPREPLY=( $(compgen -W "--help -h" -- "$cur") )
            elif [ "$COMP_CWORD" -eq 2 ]; then
                COMPREPLY=( $(compgen -W "$(_fa_alias_names)" -- "$cur") )
            fi
            return 0 # further args pass through to the alias command
            ;;
        self-update)
            COMPREPLY=( $(compgen -W "-d --dry-run --help -h" -- "$cur") )
            return 0
            ;;
        self-uninstall)
            COMPREPLY=( $(compgen -W "-y --yes -n --no -d --dry-run --help -h" -- "$cur") )
            return 0
            ;;
        --help|--version|-h|-v)
            return 0
            ;;
        *)
            # Direct alias invocation (`fa <name> …`): complete alias names first.
            if [ "$COMP_CWORD" -eq 1 ]; then
                COMPREPLY=( $(compgen -W "$(_fa_alias_names)" -- "$cur") )
            fi
            return 0
            ;;
    esac
}

complete -F _fa_complete -o default fa
