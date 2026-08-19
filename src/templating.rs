//! Templating engine for `{{var}}` placeholder substitution.

use std::collections::HashMap;

/// Substitutes `{{key}}` placeholders in the input string using the provided variable map.
/// Returns a copy of the input with all recognized placeholders replaced.
pub fn substitute(input: &str, vars: &HashMap<String, String>) -> String {
    if !input.contains("{{") {
        return input.to_string();
    }
    let mut result = input.to_string();
    for (key, value) in vars {
        let placeholder = format!("{{{{{key}}}}}");
        result = result.replace(&placeholder, value);
    }
    result
}

/// Quotes a value so it is treated as a single literal shell word when
/// interpolated into a command executed via `sh -c`. Neutralizes injection
/// attempts via `;`, `&&`, `$(...)`, backticks, etc.
pub fn shell_quote(value: &str) -> String {
    let escaped = value.replace('\'', "'\\''");
    format!("'{escaped}'")
}

/// Substitutes positional arguments (`$1`, `$2`, `${1}`, `${1:-default}`, `{{1}}`, `{{1:-default}}`, `$@`, `$*`)
/// into a shell command template, quoting each substituted value to prevent injection.
/// If no positional variables are present in the command template, any provided
/// arguments are appended at the end as shell-quoted passthrough arguments.
pub fn substitute_command_args(command: &str, args: &[String]) -> String {
    let mut result = command.to_string();

    let has_bash_positional = (1..=20).any(|i| {
        result.contains(&format!("${i}")) || result.contains(&format!("${{{i}"))
    });
    let has_template_positional = (1..=20).any(|i| {
        result.contains(&format!("{{{{{i}"))
    });
    let has_all_args = result.contains("$@") || result.contains("$*");

    if !has_bash_positional && !has_template_positional && !has_all_args {
        if args.is_empty() {
            return result;
        }
        let quoted_args = args
            .iter()
            .map(|a| shell_quote(a))
            .collect::<Vec<_>>()
            .join(" ");
        return format!("{result} {quoted_args}");
    }

    // Replace all-args placeholders: $@, $*
    if result.contains("$@") || result.contains("$*") {
        let all_quoted = args
            .iter()
            .map(|a| shell_quote(a))
            .collect::<Vec<_>>()
            .join(" ");
        result = result.replace("\"$@\"", &all_quoted);
        result = result.replace("\"$*\"", &all_quoted);
        result = result.replace("$@", &all_quoted);
        result = result.replace("$*", &all_quoted);
    }

    // Replace positional placeholders with defaults or given arguments (up to 20 parameters)
    for pos in 1..=20 {
        let given_val = if pos <= args.len() {
            Some(&args[pos - 1])
        } else {
            None
        };

        // 1. Process ${pos:-default}, ${pos:default} and quoted versions
        for separator in [":-", ":"] {
            let prefix_braced = format!("${{{pos}{separator}");
            while let Some(start) = result.find(&prefix_braced) {
                if let Some(end) = result[start + prefix_braced.len()..].find('}') {
                    let default_val = &result[start + prefix_braced.len()..start + prefix_braced.len() + end];
                    let effective_val = given_val.map(|s| s.as_str()).unwrap_or(default_val);
                    let quoted = shell_quote(effective_val);
                    let full_pattern = format!("${{{pos}{separator}{default_val}}}");
                    let quoted_pattern = format!("\"${{{pos}{separator}{default_val}}}\"");

                    if result.contains(&quoted_pattern) {
                        result = result.replace(&quoted_pattern, &quoted);
                    } else {
                        result = result.replace(&full_pattern, &quoted);
                    }
                } else {
                    break;
                }
            }
        }

        // 2. Process {{pos:-default}} and {{pos:default}}
        for separator in [":-", ":"] {
            let prefix_template = format!("{{{{{pos}{separator}");
            while let Some(start) = result.find(&prefix_template) {
                if let Some(end) = result[start + prefix_template.len()..].find("}}") {
                    let default_val = &result[start + prefix_template.len()..start + prefix_template.len() + end];
                    let effective_val = given_val.map(|s| s.as_str()).unwrap_or(default_val);
                    let quoted = shell_quote(effective_val);
                    let full_pattern = format!("{{{{{pos}{separator}{default_val}}}}}");
                    result = result.replace(&full_pattern, &quoted);
                } else {
                    break;
                }
            }
        }

        // 3. Process standard positional placeholders ($pos, ${pos}, {{pos}}) if argument exists
        if let Some(val) = given_val {
            let quoted = shell_quote(val);

            // Quoted patterns: "$1", "${1}"
            result = result.replace(&format!("\"${pos}\""), &quoted);
            result = result.replace(&format!("\"${{{pos}}}\""), &quoted);

            // Unquoted patterns: $1, ${1}
            result = result.replace(&format!("${pos}"), &quoted);
            result = result.replace(&format!("${{{pos}}}"), &quoted);

            // Template pattern: {{1}}
            result = result.replace(&format!("{{{{{pos}}}}}"), &quoted);
        }
    }

    result
}

/// Substitutes `{{key}}` placeholders in a shell command, shell-quoting every
/// substituted value to prevent command injection (CWE-78).
pub fn substitute_shell(input: &str, vars: &HashMap<String, String>) -> String {
    if !input.contains("{{") {
        return input.to_string();
    }
    let mut result = input.to_string();
    for (key, value) in vars {
        let placeholder = format!("{{{{{key}}}}}");
        result = result.replace(&placeholder, &shell_quote(value));
    }
    result
}

/// Finds all unique `{{key}}` placeholders present in the input string.
/// Unknown variables (not in `vars`) are returned so callers can prompt for them.
pub fn find_unknown_placeholders(input: &str, vars: &HashMap<String, String>) -> Vec<String> {
    let mut unknowns = Vec::new();
    let bytes = input.as_bytes();
    let mut i = 0;

    while i + 1 < bytes.len() {
        if bytes[i] == b'{' && bytes[i + 1] == b'{'
            && let Some(end) = input[i + 2..].find("}}") {
                let key = &input[i + 2..i + 2 + end];
                if !key.is_empty() && !vars.contains_key(key) && !unknowns.iter().any(|u| u == key) {
                    unknowns.push(key.to_string());
                }
                i = i + 2 + end + 2;
                continue;
            }
        i += 1;
    }

    unknowns
}

/// Resolves all placeholders across a set of inputs, prompting via the provided
/// resolver closure for any variable that has no known value yet.
pub fn resolve_all(
    inputs: &[String],
    vars: &mut HashMap<String, String>,
    defaults: &HashMap<String, String>,
    mut resolver: impl FnMut(&str) -> String,
) {
    for input in inputs {
        let unknowns = find_unknown_placeholders(input, vars);
        for key in unknowns {
            let value = if let Some(default) = defaults.get(&key) {
                let answer = resolver(&key);
                if answer.trim().is_empty() {
                    default.clone()
                } else {
                    answer
                }
            } else {
                resolver(&key)
            };
            vars.insert(key, value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholder_substitution_should_replace_known_vars() {
        println!("\n🔍 [TEST] Templating Placeholder Substitution");
        println!("   Explanation: Verifies that {{var}} placeholders are substituted in content and paths.");

        let mut vars = HashMap::new();
        vars.insert("name".to_string(), "myapp".to_string());

        let out = substitute("pnpm create my-recipe@latest {{name}} --template basics", &vars);
        assert_eq!(out, "pnpm create my-recipe@latest myapp --template basics");
        println!("   ✓ Known variable {{name}} substituted correctly: {out}\n");

        let out2 = substitute("src/pages/{{name}}.ts", &vars);
        assert_eq!(out2, "src/pages/myapp.ts");
        println!("   ✓ Path placeholder substituted correctly.\n");
    }

    #[test]
    fn find_unknown_placeholders_should_return_only_missing_keys() {
        let mut vars = HashMap::new();
        vars.insert("name".to_string(), "app".to_string());

        let input = "{{name}} {{author}} {{name}}".to_string();
        let unknowns = find_unknown_placeholders(&input, &vars);
        assert_eq!(unknowns, vec!["author"]);
    }

    #[test]
    fn shell_quote_should_wrap_in_single_quotes() {
        assert_eq!(shell_quote("myapp"), "'myapp'");
        assert_eq!(shell_quote("my app"), "'my app'");
        assert_eq!(shell_quote("it's"), "'it'\\''s'");
    }

    #[test]
    fn substitute_shell_should_neutralize_injection() {
        let mut vars = HashMap::new();
        vars.insert("name".to_string(), "myapp; rm -rf ~".to_string());

        let out = substitute_shell("pnpm create my-recipe@latest {{name}} --template minimal", &vars);
        assert_eq!(out, "pnpm create my-recipe@latest 'myapp; rm -rf ~' --template minimal");
        assert!(!out.contains("; rm -rf") || out.contains("'myapp; rm -rf ~'"));

        let out2 = substitute_shell("echo {{name}}", &vars);
        assert_eq!(out2, "echo 'myapp; rm -rf ~'");
    }

    #[test]
    fn resolve_all_with_defaults_and_interactive_resolver() {
        let inputs = vec!["{{name}}".to_string(), "{{port}}".to_string()];
        let mut vars = HashMap::new();
        let mut defaults = HashMap::new();
        defaults.insert("port".to_string(), "3000".to_string());

        // Resolver returns "my-app" for name, and empty string "" for port (to trigger default fallback)
        resolve_all(&inputs, &mut vars, &defaults, |key| {
            if key == "name" {
                "my-app".to_string()
            } else {
                "".to_string()
            }
        });

        assert_eq!(vars.get("name"), Some(&"my-app".to_string()));
        assert_eq!(vars.get("port"), Some(&"3000".to_string()), "Empty answer must fallback to default");
    }

    #[test]
    fn substitute_command_args_should_replace_bash_positional_variables() {
        let cmd = "avifenc -s 0 -q 50 $1 -o $2";
        let args = vec!["in.jpg".to_string(), "out.avif".to_string()];
        let out = substitute_command_args(cmd, &args);
        assert_eq!(out, "avifenc -s 0 -q 50 'in.jpg' -o 'out.avif'");
    }

    #[test]
    fn substitute_command_args_should_replace_braced_positional_variables() {
        let cmd = "avifenc -s 0 -q 50 {{1}} -o {{2}}";
        let args = vec!["in.jpg".to_string(), "out.avif".to_string()];
        let out = substitute_command_args(cmd, &args);
        assert_eq!(out, "avifenc -s 0 -q 50 'in.jpg' -o 'out.avif'");
    }

    #[test]
    fn substitute_command_args_should_fallback_to_passthrough_if_no_positional_vars() {
        let cmd = "avifenc -s 0 -q 50";
        let args = vec!["in.jpg".to_string(), "out.avif".to_string()];
        let out = substitute_command_args(cmd, &args);
        assert_eq!(out, "avifenc -s 0 -q 50 'in.jpg' 'out.avif'");
    }

    #[test]
    fn substitute_command_args_should_use_default_values_when_arguments_are_omitted() {
        // Bash style ${3:-40} and template style {{2:-out.avif}}
        let cmd = "avifenc -s 0 -q ${3:-40} $1 -o ${2:-default.avif}";
        let args = vec!["ticket.jpg".to_string()];
        let out = substitute_command_args(cmd, &args);
        assert_eq!(out, "avifenc -s 0 -q '40' 'ticket.jpg' -o 'default.avif'");

        // When argument is provided, override default
        let args_full = vec!["ticket.jpg".to_string(), "custom.avif".to_string(), "60".to_string()];
        let out_full = substitute_command_args(cmd, &args_full);
        assert_eq!(out_full, "avifenc -s 0 -q '60' 'ticket.jpg' -o 'custom.avif'");
    }

    #[test]
    fn substitute_command_args_should_support_colon_only_default_syntax() {
        let cmd = "avifenc -s 0 -q 50 $1 ${2:output.avif} {{3:80}}";
        let args = vec!["ticket-miduconf-2024.jpeg".to_string()];
        let out = substitute_command_args(cmd, &args);
        assert_eq!(
            out,
            "avifenc -s 0 -q 50 'ticket-miduconf-2024.jpeg' 'output.avif' '80'"
        );
    }
}
