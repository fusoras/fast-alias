/// Shell completion script generation for `fa completions <shell>`.
///
/// Static CLI-surface completions only (Recipe Player Principle: no
/// stack-specific logic). Dynamic recipe-name completion is intentionally
/// out of scope.
use anyhow::Result;

/// Shells supported by `fa completions`.
pub const SUPPORTED_SHELLS: &[&str] = &["bash", "zsh", "fish", "powershell", "elvish"];

/// Validates a shell name (case-insensitive) and returns its canonical form.
///
/// Errors with a clear message listing all supported shells.
pub fn parse_shell(shell: &str) -> Result<String> {
    let normalized = shell.to_ascii_lowercase();
    if SUPPORTED_SHELLS.contains(&normalized.as_str()) {
        return Ok(normalized);
    }
    anyhow::bail!(
        "Unknown shell '{shell}'. Supported shells: {}",
        SUPPORTED_SHELLS.join(", ")
    );
}

/// Generates the completion script for `shell` from the real `fa` CLI
/// definition. Output is deterministic for a given shell.
pub fn generate_completion(cmd: &mut clap::Command, shell: &str) -> Result<String> {
    let shell = parse_shell(shell)?;
    let mut buf = Vec::new();
    match shell.as_str() {
        "bash" => clap_complete::generate(clap_complete::shells::Bash, cmd, "fa", &mut buf),
        "zsh" => clap_complete::generate(clap_complete::shells::Zsh, cmd, "fa", &mut buf),
        "fish" => clap_complete::generate(clap_complete::shells::Fish, cmd, "fa", &mut buf),
        "powershell" => {
            clap_complete::generate(clap_complete::shells::PowerShell, cmd, "fa", &mut buf);
        }
        "elvish" => clap_complete::generate(clap_complete::shells::Elvish, cmd, "fa", &mut buf),
        _ => unreachable!("parse_shell guarantees a supported shell"),
    }
    Ok(String::from_utf8(buf)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    fn test_cmd() -> clap::Command {
        crate::Cli::command()
    }

    #[test]
    fn completions_bash_should_mention_fa_new() {
        let out = generate_completion(&mut test_cmd(), "bash").expect("bash completions should generate");
        assert!(out.contains("fa"), "bash script should mention fa");
        assert!(out.contains("new"), "bash script should contain `new` subcommand");
    }

    #[test]
    fn completions_zsh_should_contain_subcommands() {
        let out = generate_completion(&mut test_cmd(), "zsh").expect("zsh completions should generate");
        for cmd in ["new", "list", "search", "show", "alias", "self-update", "self-uninstall", "completions"] {
            assert!(out.contains(cmd), "zsh script should contain `{cmd}`");
        }
    }

    #[test]
    fn completions_fish_should_be_non_empty() {
        let out = generate_completion(&mut test_cmd(), "fish").expect("fish completions should generate");
        assert!(!out.trim().is_empty(), "fish script must be non-empty");
    }

    #[test]
    fn completions_powershell_and_elvish_should_be_non_empty() {
        for shell in ["powershell", "elvish"] {
            let out =
                generate_completion(&mut test_cmd(), shell).expect("{shell} completions should generate");
            assert!(!out.trim().is_empty(), "{shell} script must be non-empty");
        }
    }

    #[test]
    fn completions_unknown_shell_should_error_with_supported_list() {
        let err = generate_completion(&mut test_cmd(), "tcsh").expect_err("unknown shell must error");
        let msg = format!("{err:#}");
        for shell in SUPPORTED_SHELLS {
            assert!(msg.contains(shell), "error should list supported shell `{shell}`: got: {msg}");
        }
    }

    #[test]
    fn completions_output_should_be_deterministic() {
        let a = generate_completion(&mut test_cmd(), "bash").expect("bash completions should generate");
        let b = generate_completion(&mut test_cmd(), "bash").expect("bash completions should generate");
        assert!(!a.trim().is_empty(), "completion output must be non-empty");
        assert_eq!(a, b, "completion output must be stable/deterministic");
    }
}
