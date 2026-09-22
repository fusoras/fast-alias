#compdef fa
# fa zsh completion (static — installed once, no `fa completions` subcommand).
# Only deps: the `fa` binary on PATH + sed/grep/tr/sort.
# Caveat: `fa list` hides non-scaffold recipes (no create/files/steps), so those
# never appear as candidates. Reinstall this file if the CLI surface changes.

_fa_strip_ansi() {
    sed -e 's/\x1b\[[0-9;]*m//g'
}

_fa_recipes() {
    command -v fa >/dev/null 2>&1 || return 0
    fa list 2>/dev/null \
        | _fa_strip_ansi \
        | sed -n '/^Recipes:/,/^Aliases:/p' \
        | grep -v '^Recipes:' \
        | grep -v '^Aliases:' \
        | grep -v 'Usage:' \
        | sed -e 's/·.*//' -e 's/^ *//' -e 's/,/ /g' \
        | tr -s ' ' '\n' \
        | grep -v '^$' \
        | sort -u
}

_fa_alias_names() {
    command -v fa >/dev/null 2>&1 || return 0
    fa list 2>/dev/null \
        | _fa_strip_ansi \
        | sed -n '/^Aliases:/,$p' \
        | grep '^    [^ ]' \
        | grep -v ':$' \
        | sed -e 's/·.*//' -e 's/^ *//' -e 's/,/ /g' \
        | tr -s ' ' '\n' \
        | grep -v '^$' \
        | sort -u
}

_fa() {
    local curcontext="$curcontext" state line
    typeset -A opt_args
    local -a subcommands
    subcommands=(
        'new:Scaffold a new project from a recipe'
        'list:List available recipes and aliases'
        'search:Search recipes and aliases by keyword'
        'show:Show full details of a recipe or alias'
        'alias:Run an executable command from the catalog'
        'self-update:Update the fa binary in-place'
        'self-uninstall:Remove the fa binary, state and config'
    )

    _arguments -C \
        '-v[Print version]' '--version[Print version]' \
        '(-h --help)'{-h,--help}'[Print help]' \
        '1:command:->cmd' \
        '*::args:->args' && return 0

    case "$state" in
        cmd)
            _describe -t commands 'fa commands' subcommands
            ;;
        args)
            case "${line[1]}" in
                new|-n)
                    _arguments \
                        '(-v --variant)'{-v,--variant}'[Toolchain variant]:variant:' \
                        '(-d --dry-run)'{-d,--dry-run}'[Preview without making changes]' \
                        '--no-install[Skip dependency installation]' \
                        '(-h --help)'{-h,--help}'[Print help]' \
                        '1:recipe:($(_fa_recipes))' \
                        '2:name:_files'
                    ;;
                list|-l)
                    _arguments \
                        '(-s --show-hidden)'{-s,--show-hidden}'[Display unsupported recipes too]' \
                        '(-h --help)'{-h,--help}'[Print help]'
                    ;;
                search)
                    _arguments \
                        '(-h --help)'{-h,--help}'[Print help]' \
                        '1:query:($(_fa_recipes) $(_fa_alias_names))'
                    ;;
                show)
                    _arguments \
                        '(-h --help)'{-h,--help}'[Print help]' \
                        '1:target:($(_fa_recipes) $(_fa_alias_names))'
                    ;;
                alias|-a)
                    _arguments \
                        '(-h --help)'{-h,--help}'[Print help]' \
                        '1:name:($(_fa_alias_names))' \
                        '*::args:_default'
                    ;;
                self-update)
                    _arguments \
                        '(-d --dry-run)'{-d,--dry-run}'[Preview without modifying binary]' \
                        '(-h --help)'{-h,--help}'[Print help]'
                    ;;
                self-uninstall)
                    _arguments \
                        '(-y --yes)'{-y,--yes}'[Confirm removal of config and state]' \
                        '(-n --no)'{-n,--no}'[Keep config and state]' \
                        '(-d --dry-run)'{-d,--dry-run}'[Preview without deleting files]' \
                        '(-h --help)'{-h,--help}'[Print help]'
                    ;;
            esac
            ;;
    esac
}

(( $+functions[compdef] )) && compdef _fa fa
