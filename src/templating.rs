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

fn find_matching_brace(s: &str) -> Option<usize> {
    let mut depth = 1usize;
    for (idx, ch) in s.char_indices() {
        if ch == '{' {
            depth += 1;
        } else if ch == '}' {
            depth -= 1;
            if depth == 0 {
                return Some(idx);
            }
        }
    }
    None
}

fn find_matching_double_brace(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut depth = 0usize;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' {
            depth += 1;
            i += 1;
        } else if bytes[i] == b'}' {
            if depth > 0 {
                depth -= 1;
                i += 1;
            } else if i + 1 < bytes.len() && bytes[i + 1] == b'}' {
                return Some(i);
            } else {
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    None
}

/// Resolves auto-increment file patterns within fallback defaults.
///
/// Supported patterns within `{+...}`:
/// - `{+#}`, `{+##}`, `{+###}` for numeric sequences (01 to 99).
/// - `{+A}`, `{+AA}` for uppercase alphabetic sequences (A to Z, AA to ZZ).
/// - `{+a}`, `{+aa}` for lowercase alphabetic sequences (a to z, aa to zz).
/// - Mixed combinations like `{+Aa}` (Aa to Zz), `{+A#}` (A1 to Z9), etc.
/// - Optional strategy modifier: `{+##:grow}`, `{+##:rotate}`, `{+##:error}`.
pub fn resolve_auto_increment(input: &str, default_overflow: Option<&str>) -> String {
    let Some(start_rel) = input.find("{+") else {
        return input.to_string();
    };

    let Some(end_rel) = input[start_rel..].find('}') else {
        return input.to_string();
    };

    let token_content = &input[start_rel + 2..start_rel + end_rel];
    let (pattern, token_strategy) = match token_content.split_once(':') {
        Some((p, s)) => (p, Some(s)),
        None => (token_content, None),
    };

    if pattern.is_empty() || !pattern.chars().all(|c| c == '#' || c == 'A' || c == 'a') {
        return input.to_string();
    }

    let strategy = token_strategy
        .or(default_overflow)
        .unwrap_or("overwrite");

    let prefix = &input[..start_rel];
    let suffix = &input[start_rel + end_rel + 1..];

    let pattern_chars: Vec<char> = pattern.chars().collect();
    let is_pure_numeric = pattern_chars.iter().all(|&c| c == '#');

    let format_token = |k: u64| -> String {
        if is_pure_numeric {
            format!("{:0width$}", k, width = pattern_chars.len())
        } else {
            let mut res = vec![' '; pattern_chars.len()];
            let mut rem = k;
            for i in (0..pattern_chars.len()).rev() {
                match pattern_chars[i] {
                    '#' => {
                        let digit = (rem % 10) as u8;
                        res[i] = (b'0' + digit) as char;
                        rem /= 10;
                    }
                    'A' => {
                        let digit = (rem % 26) as u8;
                        res[i] = (b'A' + digit) as char;
                        rem /= 26;
                    }
                    'a' => {
                        let digit = (rem % 26) as u8;
                        res[i] = (b'a' + digit) as char;
                        rem /= 26;
                    }
                    _ => unreachable!(),
                }
            }
            res.into_iter().collect::<String>()
        }
    };

    let (start_idx, max_idx): (u64, u64) = if is_pure_numeric {
        let width = pattern_chars.len() as u32;
        let max_val = 10u64.saturating_pow(width).saturating_sub(1);
        (1, max_val)
    } else {
        let mut total = 1u64;
        for &c in &pattern_chars {
            let base = if c == '#' { 10 } else { 26 };
            total = total.saturating_mul(base);
        }
        (0, total.saturating_sub(1))
    };

    let initial_token = format_token(start_idx);
    let initial_candidate = format!("{prefix}{initial_token}{suffix}");

    if !std::path::Path::new(&initial_candidate).exists() {
        return initial_candidate;
    }

    // Initial candidate exists. Find next available slot
    for idx in (start_idx + 1)..=max_idx {
        let token_str = format_token(idx);
        let candidate = format!("{prefix}{token_str}{suffix}");
        if !std::path::Path::new(&candidate).exists() {
            eprintln!("[fa] '{initial_candidate}' already exists → using '{candidate}'");
            return candidate;
        }
    }

    // Exhausted capacity -> overflow strategy
    let last_token = format_token(max_idx);
    let last_candidate = format!("{prefix}{last_token}{suffix}");

    match strategy.to_lowercase().as_str() {
        "grow" => {
            if is_pure_numeric {
                let mut num = max_idx + 1;
                let limit = max_idx.saturating_add(10_000);
                while num < limit {
                    let candidate = format!("{prefix}{num}{suffix}");
                    if !std::path::Path::new(&candidate).exists() {
                        eprintln!("[fa] '{initial_candidate}' already exists → using '{candidate}'");
                        return candidate;
                    }
                    num += 1;
                }
            }
            eprintln!("[fa] Warning: Limit reached for '{{+{pattern}}}' ({last_token}). Overwriting '{last_candidate}'");
            last_candidate
        }
        "rotate" => {
            let start_token = format_token(start_idx);
            let start_candidate = format!("{prefix}{start_token}{suffix}");
            eprintln!("[fa] Warning: Limit reached for '{{+{pattern}}}' ({last_token}). Rotating to '{start_candidate}'");
            start_candidate
        }
        "error" => {
            eprintln!("[fa] Error: Limit reached for pattern '{{+{pattern}}}' ({last_token})");
            last_candidate
        }
        _ /* "overwrite" */ => {
            eprintln!("[fa] Warning: Limit reached for '{{+{pattern}}}' ({last_token}). Overwriting '{last_candidate}'");
            last_candidate
        }
    }
}

/// Substitutes positional arguments (`$1`, `$2`, `${1}`, `${1:-default}`, `{{1}}`, `{{1:-default}}`, `$@`, `$*`)
/// into a shell command template, quoting each substituted value to prevent injection.
/// If no positional variables are present in the command template, any provided
/// arguments are appended at the end as shell-quoted passthrough arguments.
#[allow(dead_code)]
pub fn substitute_command_args(command: &str, args: &[String]) -> String {
    substitute_command_args_with_overflow(command, args, None)
}

/// Same as [`substitute_command_args`] with an explicit default overflow strategy
/// for auto-increment file fallback placeholders.
pub fn substitute_command_args_with_overflow(
    command: &str,
    args: &[String],
    default_overflow: Option<&str>,
) -> String {
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
                if let Some(end) = find_matching_brace(&result[start + prefix_braced.len()..]) {
                    let default_val = &result[start + prefix_braced.len()..start + prefix_braced.len() + end];
                    let effective_val = match given_val {
                        Some(val) => val.as_str().to_string(),
                        None => resolve_auto_increment(default_val, default_overflow),
                    };
                    let quoted = shell_quote(&effective_val);
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
                if let Some(end) = find_matching_double_brace(&result[start + prefix_template.len()..]) {
                    let default_val = &result[start + prefix_template.len()..start + prefix_template.len() + end];
                    let effective_val = match given_val {
                        Some(val) => val.as_str().to_string(),
                        None => resolve_auto_increment(default_val, default_overflow),
                    };
                    let quoted = shell_quote(&effective_val);
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

    #[test]
    fn substitute_command_args_should_auto_increment_numeric_and_alpha_placeholders() {
        let tmp_dir = std::env::temp_dir().join("fa_test_autoincrement");
        let _ = std::fs::remove_dir_all(&tmp_dir);
        std::fs::create_dir_all(&tmp_dir).unwrap();

        let base_pattern = tmp_dir.join("render-{+##}.png").to_string_lossy().to_string();
        let cmd = format!("render-cli $1 -o ${{2:-{base_pattern}}}");

        // 1. First run: no files exist -> should resolve to 01
        let out1 = substitute_command_args(&cmd, &["scene".to_string()]);
        let expected1 = tmp_dir.join("render-01.png").to_string_lossy().to_string();
        assert_eq!(out1, format!("render-cli 'scene' -o '{expected1}'"));

        // Create render-01.png
        std::fs::write(&expected1, "dummy").unwrap();

        // 2. Second run: render-01.png exists -> should resolve to 02
        let out2 = substitute_command_args(&cmd, &["scene".to_string()]);
        let expected2 = tmp_dir.join("render-02.png").to_string_lossy().to_string();
        assert_eq!(out2, format!("render-cli 'scene' -o '{expected2}'"));

        // 3. User passes explicit arg -> auto-increment is bypassed
        let explicit = tmp_dir.join("custom.png").to_string_lossy().to_string();
        let out_user = substitute_command_args(&cmd, &["scene".to_string(), explicit.clone()]);
        assert_eq!(out_user, format!("render-cli 'scene' -o '{explicit}'"));

        // 4. Alpha uppercase and mixed case
        let alpha_pattern = tmp_dir.join("shot-{+A}.png").to_string_lossy().to_string();
        let cmd_alpha = format!("shot-cli ${{1:-{alpha_pattern}}}");
        let out_alpha = substitute_command_args(&cmd_alpha, &[]);
        let expected_alpha = tmp_dir.join("shot-A.png").to_string_lossy().to_string();
        assert_eq!(out_alpha, format!("shot-cli '{expected_alpha}'"));

        let mixed_pattern = tmp_dir.join("var-{+Aa}.png").to_string_lossy().to_string();
        let cmd_mixed = format!("var-cli ${{1:-{mixed_pattern}}}");
        let out_mixed = substitute_command_args(&cmd_mixed, &[]);
        let expected_mixed = tmp_dir.join("var-Aa.png").to_string_lossy().to_string();
        assert_eq!(out_mixed, format!("var-cli '{expected_mixed}'"));

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    #[test]
    fn substitute_command_args_should_handle_overflow_strategies() {
        let tmp_dir = std::env::temp_dir().join("fa_test_overflow");
        let _ = std::fs::remove_dir_all(&tmp_dir);
        std::fs::create_dir_all(&tmp_dir).unwrap();

        // Populate items 1 through 9
        for i in 1..=9 {
            std::fs::write(tmp_dir.join(format!("slot-{i}.log")), "x").unwrap();
        }

        let p_default = tmp_dir.join("slot-{+#}.log").to_string_lossy().to_string();
        let cmd_default = format!("app ${{1:-{p_default}}}");
        // 1. Default (overwrite): returns slot-9.log
        let out_over = substitute_command_args(&cmd_default, &[]);
        let expected_over = tmp_dir.join("slot-9.log").to_string_lossy().to_string();
        assert_eq!(out_over, format!("app '{expected_over}'"));

        // 2. Rotate: returns slot-1.log
        let p_rotate = tmp_dir.join("slot-{+#:rotate}.log").to_string_lossy().to_string();
        let cmd_rotate = format!("app ${{1:-{p_rotate}}}");
        let out_rotate = substitute_command_args(&cmd_rotate, &[]);
        let expected_rotate = tmp_dir.join("slot-1.log").to_string_lossy().to_string();
        assert_eq!(out_rotate, format!("app '{expected_rotate}'"));

        // 3. Grow: slot-10.log doesn't exist -> should resolve to slot-10.log
        let p_grow = tmp_dir.join("slot-{+#:grow}.log").to_string_lossy().to_string();
        let cmd_grow = format!("app ${{1:-{p_grow}}}");
        let out_grow = substitute_command_args(&cmd_grow, &[]);
        let expected_grow = tmp_dir.join("slot-10.log").to_string_lossy().to_string();
        assert_eq!(out_grow, format!("app '{expected_grow}'"));

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }
}
