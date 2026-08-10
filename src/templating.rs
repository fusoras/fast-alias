//! Templating engine for `{{var}}` placeholder substitution.

use std::collections::HashMap;

/// Substitutes `{{key}}` placeholders in the input string using the provided variable map.
/// Returns a copy of the input with all recognized placeholders replaced.
pub fn substitute(input: &str, vars: &HashMap<String, String>) -> String {
    let mut result = input.to_string();
    for (key, value) in vars {
        let placeholder = format!("{{{{{key}}}}}");
        result = result.replace(&placeholder, value);
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

        let out = substitute("pnpm create astro@latest {{name}} --template basics", &vars);
        assert_eq!(out, "pnpm create astro@latest myapp --template basics");
        println!("   ✓ Command placeholder substituted correctly.");

        let out2 = substitute("src/pages/{{name}}.astro", &vars);
        assert_eq!(out2, "src/pages/myapp.astro");
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
}
