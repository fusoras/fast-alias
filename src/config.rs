use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// First-run example configuration, written to `~/.config/fa/recipes.toml`
/// when the user config directory does not exist. The user can delete it;
/// an empty catalog then shows no recipes and no aliases.
pub const EXAMPLE_CONFIG: &str = r##"#:schema https://raw.githubusercontent.com/fusoras/fast-alias/main/schema/recipe.schema.json
# fa example configuration
# Edit this file or add more .toml files under recipes.d/ to define your own
# recipes and aliases. Templates referenced as `from = "templates/<path>"`
# are resolved from ~/.config/fa/templates/<path>.

[recipes.example]
name = "Example"
description = "Example recipe — replace it with your own"
language = "shell"

# General-purpose aliases, grouped by section. Run them with `fa --alias <name>` or `fa <name>`.
[aliases.demo]
hello = { command = "echo 'Hello from fa!'", description = "Example alias" }
"##;

/// Global configuration template, written to `~/.config/fa/config.toml`
/// when initializing the example configuration directory.
pub const EXAMPLE_GLOBAL_CONFIG: &str = r##"[packs]
# Behavior when running `fa --new <recipe>` without specifying a pack or component:
# "list" (default) -> displays all available packs and components
# "default" -> automatically installs the pack specified in `default_pack` in the recipe
# "error" -> raises an error requiring an explicit pack or component
default_behavior = "list"

[alias]
# Command aliases (Git style)
# Native fa command shortcuts:
n = "--new"
l = "--list"
r = "--recipe"
rn = "--recipe new"
rv = "--recipe validate"
re = "--recipe edit"
rm = "--recipe rm"
t = "--template"
sh = "--show"
se = "--search"

# External shell command shortcuts start with '!':
# ac = "!git add -A && git commit -m"
# st = "!git status"
# b = "!git branch"
# s = "!git switch"
"##;

/// File generation specification. Each entry is one of:
/// - `{ from = "templates/my-recipe/.prettierrc" }`   copy static file
/// - `{ inline = "..." }`                          write content verbatim
/// - `{ template = "templates/x.tpl" }`            copy file AND substitute {{var}}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FileSpec {
    pub from: Option<String>,
    pub inline: Option<String>,
    pub template: Option<String>,
    pub skip_if_exists: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Create {
    pub command: Option<String>,
    pub template_dir: Option<String>,
}

/// A file-copy step: copies a template directory to a destination.
/// Used by the packs system to deploy web components.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CreateStep {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Pm {
    pub install: Option<BTreeMap<String, String>>,
    pub dev_install: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Tool {
    pub tool: String,
    pub script: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Tooling {
    pub linter: Option<Tool>,
    pub formatter: Option<Tool>,
    pub check: Option<Tool>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Step {
    #[serde(default)]
    pub command: Option<String>,
    pub create: Option<CreateStep>,
    pub description: Option<String>,
    pub platform: Option<String>,
    #[serde(default)]
    pub install: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Variable {
    pub prompt: String,
    pub default: Option<String>,
    /// Optional type check: "string" | "integer" | "float" | "boolean".
    /// Absent = free string, no type check (legacy behavior).
    #[serde(rename = "type", default)]
    pub var_type: Option<String>,
    /// Optional allow-list: input must equal one of these strings.
    #[serde(default)]
    pub choices: Option<Vec<String>>,
    /// Optional regex the string form must match (type must be string).
    #[serde(default)]
    pub pattern: Option<String>,
    /// Optional: when true, empty input is rejected.
    #[serde(default)]
    pub required: Option<bool>,
}

impl Variable {
    /// True when the variable declares any validation rule.
    pub fn is_typed(&self) -> bool {
        self.var_type.is_some()
            || self.choices.is_some()
            || self.pattern.is_some()
            || self.required == Some(true)
    }

    /// Schema check at config-load time: unknown types, empty choices,
    /// `pattern` on non-string types and invalid regexes fail with the
    /// recipe + variable name in the message.
    pub fn validate_schema(&self, recipe: &str, name: &str) -> anyhow::Result<()> {
        if let Some(t) = &self.var_type
            && !matches!(t.as_str(), "string" | "integer" | "float" | "boolean") {
                anyhow::bail!(
                    "recipe '{recipe}' variable '{name}': unknown type '{t}' (expected one of: string, integer, float, boolean)"
                );
            }
        if let Some(choices) = &self.choices
            && choices.is_empty() {
                anyhow::bail!(
                    "recipe '{recipe}' variable '{name}': 'choices' must not be empty"
                );
            }
        if let Some(p) = &self.pattern {
            let t = self.var_type.as_deref().unwrap_or("string");
            if t != "string" {
                anyhow::bail!(
                    "recipe '{recipe}' variable '{name}': 'pattern' requires type = \"string\" (got '{t}')"
                );
            }
            regex::Regex::new(p).map_err(|e| {
                anyhow::anyhow!(
                    "recipe '{recipe}' variable '{name}': invalid pattern regex '{p}': {e}"
                )
            })?;
        }
        Ok(())
    }

    /// Runtime value check: required-emptiness, type, choices and pattern.
    /// Empty (or blank when required) values skip type/choices/pattern so an
    /// optional variable left blank stays allowed.
    pub fn validate_value(&self, name: &str, value: &str) -> Result<(), String> {
        if self.required == Some(true) && value.trim().is_empty() {
            return Err(format!("'{name}' is required and must not be empty"));
        }
        if value.is_empty() {
            return Ok(());
        }
        if let Some(t) = self.var_type.as_deref() {
            let ok = match t {
                "string" => true,
                "integer" => value.trim().parse::<i64>().is_ok(),
                "float" => value.trim().parse::<f64>().is_ok(),
                "boolean" => matches!(
                    value.trim().to_ascii_lowercase().as_str(),
                    "true" | "false" | "1" | "0" | "yes" | "no" | "y" | "n"
                ),
                unknown => {
                    return Err(format!(
                        "'{name}': unknown type '{unknown}' (expected one of: string, integer, float, boolean)"
                    ));
                }
            };
            if !ok {
                return Err(format!("'{name}': '{value}' is not a valid {t}"));
            }
        }
        if let Some(choices) = &self.choices
            && !choices.iter().any(|c| c == value) {
                return Err(format!("'{name}': '{value}' is not one of: {}", choices.join(", ")));
            }
        if let Some(p) = &self.pattern {
            let re = regex::Regex::new(p)
                .map_err(|e| format!("'{name}': invalid pattern regex '{p}': {e}"))?;
            if !re.is_match(value) {
                return Err(format!("'{name}': '{value}' does not match pattern '{p}'"));
            }
        }
        Ok(())
    }
}

/// Pack definition: a named group of components.
/// Can live in `~/.config/fa/<packs_dir>/<name>.toml`, multi-pack TOML files,
/// or directly inline in `[recipes.<name>.packs.<pack_name>]`.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct Pack {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub components: Vec<String>,
}

/// Inline pack definition inside a recipe.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct PackDefinition {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub components: Vec<String>,
}

/// Positional argument specification for an alias command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandArg {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default = "default_true")]
    pub required: bool,
}

fn default_true() -> bool {
    true
}

/// Executable command declared in the alias catalog, invoked via `fa --alias <name>` or `fa <name>`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Command {
    pub command: String,
    pub description: Option<String>,
    pub platform: Option<String>,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_args")]
    pub args: Vec<CommandArg>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub env_force: BTreeMap<String, String>,
    #[serde(skip)]
    pub source_file: Option<PathBuf>,
    #[serde(skip)]
    pub source_line: Option<usize>,
}

impl Command {
    /// Formats the explicit argument signature (e.g. `<input> [output]`).
    /// Returns `None` if no arguments are explicitly defined.
    pub fn argument_signature(&self) -> Option<String> {
        if self.args.is_empty() {
            return None;
        }
        let sig = self
            .args
            .iter()
            .map(|arg| {
                if arg.required {
                    format!("<{}>", arg.name)
                } else {
                    format!("[{}]", arg.name)
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
        Some(sig)
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Recipe {
    pub name: String,
    pub description: String,
    pub language: Option<String>,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub variants: Vec<String>,
    pub create: Option<Create>,
    pub pm: Option<Pm>,
    pub tooling: Option<Tooling>,
    /// Optional base directory under `~/.config/fa/templates/` prepended to
    /// every prefixless `from`/`template` spec (e.g. `template_base =
    /// "rust-stack"` + `from = ".gitignore"` → `rust-stack/.gitignore`).
    /// `None` (default) keeps the legacy behavior byte-identical: specs
    /// resolve exactly as `normalize_template_rel` does today.
    #[serde(default)]
    pub template_base: Option<String>,
    /// When true, the engine strips floating range prefixes (`^`/`~`) from
    /// `package.json` dependency versions after all installs complete.
    #[serde(default)]
    pub pin_versions: Option<bool>,
    #[serde(default)]
    pub files: BTreeMap<String, FileSpec>,
    #[serde(default)]
    pub variables: BTreeMap<String, Variable>,
    #[serde(default)]
    pub steps: Vec<Step>,
    /// Optional message printed after a successful `fa --new`, reminding the
    /// user of manual follow-ups (e.g. "edit 'src/config.ts'").
    #[serde(default)]
    pub final_message: Option<String>,
    /// Directory (relative to ~/.config/fa/) where pack definitions live.
    /// Packs are .toml files listing components to install together.
    #[serde(default)]
    pub packs_dir: Option<String>,
    /// Directory (relative to ~/.config/fa/) where component templates live.
    /// Overrides the default templates/ directory for this recipe's create steps.
    #[serde(default)]
    pub templates_dir: Option<String>,
    /// Default pack name used when `fa --new <recipe>` is called without
    /// specifying a component or pack.
    #[serde(default, alias = "default")]
    pub default_pack: Option<String>,
    /// Optional inline packs defined directly inside the recipe.
    #[serde(default)]
    pub packs: BTreeMap<String, PackDefinition>,
    #[serde(skip)]
    pub source_file: Option<PathBuf>,
    #[serde(skip)]
    pub source_line: Option<usize>,
}

fn default_packs_behavior() -> String {
    "list".to_string()
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct PacksSettings {
    /// Behavior when running `fa --new <recipe>` without specifying pack or component.
    /// Values: "list" (default) | "default" (installs recipe default_pack)
    #[serde(default = "default_packs_behavior")]
    pub default_behavior: String,
}

impl Default for PacksSettings {
    fn default() -> Self {
        Self {
            default_behavior: default_packs_behavior(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct GlobalConfig {
    #[serde(default)]
    pub packs: PacksSettings,
    #[serde(default, alias = "aliases")]
    pub alias: BTreeMap<String, String>,
}

/// Parses `config.toml` into [`GlobalConfig`], supporting standard quoted TOML
/// as well as Git-style unquoted alias lines (e.g. `b = branch`, `ac = !git add...`).
pub fn parse_global_config(content: &str) -> anyhow::Result<GlobalConfig> {
    if let Ok(cfg) = toml::from_str::<GlobalConfig>(content) {
        return Ok(cfg);
    }

    // Lenient fallback for Git-style unquoted alias values (e.g. b = branch, ac = !git add...)
    let mut normalized = String::with_capacity(content.len() + 64);
    let mut in_alias_section = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_alias_section = trimmed == "[alias]" || trimmed == "[aliases]";
            normalized.push_str(line);
            normalized.push('\n');
            continue;
        }

        if in_alias_section && let Some((key, val)) = trimmed.split_once('=') {
            let key = key.trim();
            let val = val.trim();
            if !val.starts_with('"') && !val.starts_with('\'') && !val.is_empty() {
                let escaped = val.replace('\\', "\\\\").replace('"', "\\\"");
                normalized.push_str(&format!("{key} = \"{escaped}\"\n"));
                continue;
            }
        }

        normalized.push_str(line);
        normalized.push('\n');
    }

    toml::from_str::<GlobalConfig>(&normalized)
        .map_err(|e| anyhow::anyhow!("Failed to parse config.toml: {e}"))
}

/// Parses pack definitions from a TOML string. Supports:
/// 1. `[packs.<name>]` tables
/// 2. `[[packs]]` array of tables
/// 3. Top-level tables where each table contains `components = [...]`
/// 4. Root table containing `components = [...]` (single pack file)
pub fn parse_packs_from_toml(content: &str, file_stem: &str) -> Vec<Pack> {
    let Ok(val) = toml::from_str::<toml::Value>(content) else {
        return Vec::new();
    };

    let Some(table) = val.as_table() else {
        return Vec::new();
    };

    if let Some(packs_val) = table.get("packs") {
        if let Some(packs_table) = packs_val.as_table() {
            let mut packs = Vec::new();
            for (key, sub_val) in packs_table {
                if let Some(sub_table) = sub_val.as_table() {
                    let name = sub_table
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or(key)
                        .to_string();
                    let desc = sub_table
                        .get("description")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    let components = extract_components(sub_table);
                    packs.push(Pack {
                        name,
                        description: desc,
                        components,
                    });
                }
            }
            return packs;
        }

        if let Some(packs_array) = packs_val.as_array() {
            let mut packs = Vec::new();
            for item in packs_array {
                if let Some(sub_table) = item.as_table() {
                    let name = sub_table
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or(file_stem)
                        .to_string();
                    let desc = sub_table
                        .get("description")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    let components = extract_components(sub_table);
                    packs.push(Pack {
                        name,
                        description: desc,
                        components,
                    });
                }
            }
            return packs;
        }
    }

    if table.contains_key("components") {
        let name = table
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or(file_stem)
            .to_string();
        let desc = table
            .get("description")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let components = extract_components(table);
        return vec![Pack {
            name,
            description: desc,
            components,
        }];
    }

    let mut top_packs = Vec::new();
    for (key, sub_val) in table {
        if let Some(sub_table) = sub_val.as_table()
            && sub_table.contains_key("components")
        {
            let name = sub_table
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or(key)
                .to_string();
            let desc = sub_table
                .get("description")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let components = extract_components(sub_table);
            top_packs.push(Pack {
                name,
                description: desc,
                components,
            });
        }
    }
    if !top_packs.is_empty() {
        return top_packs;
    }

    Vec::new()
}

fn extract_components(table: &toml::map::Map<String, toml::Value>) -> Vec<String> {
    table
        .get("components")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|val| val.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Config {
    #[serde(default)]
    pub settings: GlobalConfig,
    #[serde(default)]
    pub recipes: BTreeMap<String, Recipe>,
    /// General-purpose executable commands, organized by section. Each entry
    /// maps a section name (e.g. `git`, `sistema`) to its commands, so aliases
    /// live independently of scaffolding recipes.
    #[serde(default)]
    pub aliases: BTreeMap<String, BTreeMap<String, Command>>,
    /// Fallback environment variables declared at the namespace/section level (`_env`).
    #[serde(default)]
    pub alias_env: BTreeMap<String, BTreeMap<String, String>>,
    /// Forced/override environment variables declared at the namespace/section level (`_env_force`).
    #[serde(default)]
    pub alias_env_force: BTreeMap<String, BTreeMap<String, String>>,
    /// File-level template variables (`[vars]`).
    #[serde(default)]
    pub vars: BTreeMap<String, String>,
}

/// Flexible specification for environment variables: either a key-value map or a bash-style string (e.g. `'export FOO="bar" && export BAZ=qux'`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EnvSpec {
    Map(BTreeMap<String, String>),
    String(String),
}

impl EnvSpec {
    pub fn into_map(self) -> BTreeMap<String, String> {
        match self {
            EnvSpec::Map(m) => m,
            EnvSpec::String(s) => parse_env_string(&s),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ArgsSpec {
    Tokens(Vec<String>),
    DetailedList(Vec<RawArgItem>),
    DetailedMap(toml::Table),
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct RawArgItem {
    pub name: Option<String>,
    pub description: Option<String>,
    pub required: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum RawArgEntry {
    Object(RawArgItem),
    Description(String),
}

pub fn parse_arg_spec(
    name_str: &str,
    description: Option<String>,
    explicit_required: Option<bool>,
) -> CommandArg {
    let t = name_str.trim();
    let (clean_name, default_required) = if t.starts_with('[') && t.ends_with(']') && t.len() >= 2 {
        (t[1..t.len() - 1].trim().to_string(), false)
    } else if t.starts_with('<') && t.ends_with('>') && t.len() >= 2 {
        (t[1..t.len() - 1].trim().to_string(), true)
    } else {
        (t.to_string(), true)
    };

    let required = explicit_required.unwrap_or(default_required);
    CommandArg {
        name: clean_name,
        description: description.map(|d| d.trim().to_string()).filter(|d| !d.is_empty()),
        required,
    }
}

impl ArgsSpec {
    pub fn into_command_args(self) -> anyhow::Result<Vec<CommandArg>> {
        match self {
            ArgsSpec::Tokens(tokens) => {
                let args = tokens
                    .into_iter()
                    .map(|tok| parse_arg_spec(&tok, None, None))
                    .collect();
                Ok(args)
            }
            ArgsSpec::DetailedList(items) => {
                let mut args = Vec::new();
                for (idx, item) in items.into_iter().enumerate() {
                    let name = item
                        .name
                        .as_deref()
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .ok_or_else(|| {
                            anyhow::anyhow!(
                                "alias argument at index {} in list must have a non-empty 'name'",
                                idx + 1
                            )
                        })?;
                    args.push(parse_arg_spec(name, item.description, item.required));
                }
                Ok(args)
            }
            ArgsSpec::DetailedMap(table) => {
                let all_numeric =
                    !table.is_empty() && table.keys().all(|k| k.parse::<usize>().is_ok());
                let mut args = Vec::new();
                if all_numeric {
                    let mut entries: Vec<(usize, toml::Value)> = Vec::new();
                    for (k, v) in table {
                        let idx = k.parse::<usize>().unwrap();
                        entries.push((idx, v));
                    }
                    entries.sort_by_key(|&(idx, _)| idx);
                    for (idx, val) in entries {
                        let entry: RawArgEntry = val.try_into().map_err(|e| {
                            anyhow::anyhow!(
                                "failed to parse argument definition at index {idx}: {e}"
                            )
                        })?;
                        let (name, desc, req) = match entry {
                            RawArgEntry::Object(item) => (
                                item.name.unwrap_or_else(|| format!("arg{idx}")),
                                item.description,
                                item.required,
                            ),
                            RawArgEntry::Description(d) => (format!("arg{idx}"), Some(d), None),
                        };
                        args.push(parse_arg_spec(&name, desc, req));
                    }
                } else {
                    for (key, val) in table {
                        let entry: RawArgEntry = val.try_into().map_err(|e| {
                            anyhow::anyhow!("failed to parse argument definition '{key}': {e}")
                        })?;
                        let (name, desc, req) = match entry {
                            RawArgEntry::Object(item) => (
                                item.name.unwrap_or(key),
                                item.description,
                                item.required,
                            ),
                            RawArgEntry::Description(d) => (key, Some(d), None),
                        };
                        args.push(parse_arg_spec(&name, desc, req));
                    }
                }
                Ok(args)
            }
        }
    }
}

fn deserialize_args<'de, D>(deserializer: D) -> Result<Vec<CommandArg>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let opt = Option::<ArgsSpec>::deserialize(deserializer)?;
    match opt {
        Some(spec) => spec.into_command_args().map_err(serde::de::Error::custom),
        None => Ok(Vec::new()),
    }
}

#[derive(Deserialize)]
struct RawCommand {
    command: Option<String>,
    description: Option<String>,
    platform: Option<String>,
    #[serde(default)]
    aliases: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_args")]
    args: Vec<CommandArg>,
    #[serde(default)]
    env: Option<EnvSpec>,
    #[serde(default)]
    env_force: Option<EnvSpec>,
}

#[derive(Deserialize)]
struct RawConfig {
    #[serde(default)]
    settings: GlobalConfig,
    #[serde(default)]
    recipes: BTreeMap<String, Recipe>,
    #[serde(default)]
    vars: BTreeMap<String, String>,
    #[serde(default)]
    aliases: BTreeMap<String, BTreeMap<String, toml::Value>>,
    #[serde(default)]
    pub alias: BTreeMap<String, BTreeMap<String, toml::Value>>,
}

impl<'de> Deserialize<'de> for Config {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = RawConfig::deserialize(deserializer)?;
        let mut config = Config {
            settings: raw.settings,
            recipes: raw.recipes,
            aliases: BTreeMap::new(),
            alias_env: BTreeMap::new(),
            alias_env_force: BTreeMap::new(),
            vars: raw.vars.clone(),
        };

        let mut section_vars: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();

        let mut raw_aliases = raw.aliases;
        for (sec, entries) in raw.alias {
            raw_aliases.entry(sec).or_default().extend(entries);
        }

        for (section, entries) in raw_aliases {
            for (key, val) in entries {
                if key == "_env" {
                    let spec: EnvSpec = val.try_into().map_err(serde::de::Error::custom)?;
                    config.alias_env.entry(section.clone()).or_default().extend(spec.into_map());
                } else if key == "_env_force" {
                    let spec: EnvSpec = val.try_into().map_err(serde::de::Error::custom)?;
                    config.alias_env_force.entry(section.clone()).or_default().extend(spec.into_map());
                } else if key == "_vars" {
                    let vars_map: BTreeMap<String, String> = val.try_into().map_err(serde::de::Error::custom)?;
                    section_vars.entry(section.clone()).or_default().extend(vars_map);
                } else {
                    let raw_cmd: RawCommand = val.try_into().map_err(serde::de::Error::custom)?;
                    let cmd = Command {
                        command: raw_cmd.command.unwrap_or_default(),
                        description: raw_cmd.description,
                        platform: raw_cmd.platform,
                        aliases: raw_cmd.aliases,
                        args: raw_cmd.args,
                        env: raw_cmd.env.map(|e| e.into_map()).unwrap_or_default(),
                        env_force: raw_cmd.env_force.map(|e| e.into_map()).unwrap_or_default(),
                        source_file: None,
                        source_line: None,
                    };
                    config.aliases.entry(section.clone()).or_default().insert(key, cmd);
                }
            }
        }

        // Apply template variable substitutions (_vars and [vars])
        for (section, commands) in &mut config.aliases {
            let mut effective_vars = raw.vars.clone();
            if let Some(sv) = section_vars.get(section) {
                effective_vars.extend(sv.clone());
            }

            for _ in 0..5 {
                let snapshot = effective_vars.clone();
                for v in effective_vars.values_mut() {
                    for (sk, sv) in &snapshot {
                        let placeholder = format!("{{{{{sk}}}}}");
                        if v.contains(&placeholder) {
                            *v = v.replace(&placeholder, sv);
                        }
                    }
                }
            }

            for cmd in commands.values_mut() {
                for (k, v) in &effective_vars {
                    let placeholder = format!("{{{{{k}}}}}");
                    if cmd.command.contains(&placeholder) {
                        cmd.command = cmd.command.replace(&placeholder, v);
                    }
                    if let Some(desc) = &mut cmd.description
                        && desc.contains(&placeholder)
                    {
                        *desc = desc.replace(&placeholder, v);
                    }
                    for arg in &mut cmd.args {
                        if let Some(desc) = &mut arg.description
                            && desc.contains(&placeholder)
                        {
                            *desc = desc.replace(&placeholder, v);
                        }
                    }
                    for env_val in cmd.env.values_mut() {
                        if env_val.contains(&placeholder) {
                            *env_val = env_val.replace(&placeholder, v);
                        }
                    }
                    for env_force_val in cmd.env_force.values_mut() {
                        if env_force_val.contains(&placeholder) {
                            *env_force_val = env_force_val.replace(&placeholder, v);
                        }
                    }
                }
            }
        }

        Ok(config)
    }
}

impl Config {
    /// Fallback section label used only when a TOML section name arrives
    /// empty/whitespace. Real sections always come from `[aliases.<name>]`
    /// sections at runtime (Recipe Player Principle); nothing is hardcoded.
    pub const FALLBACK_ALIAS_SECTION: &'static str = "general";

    /// Returns the display name for an alias section, falling back to
    /// [`Self::FALLBACK_ALIAS_SECTION`] for empty/whitespace names.
    pub fn display_section(section: &str) -> &str {
        if section.trim().is_empty() {
            Self::FALLBACK_ALIAS_SECTION
        } else {
            section
        }
    }

    /// Synchronizes Git-style shell aliases (prefixed with '!') from `settings.alias`
    /// into `self.aliases["config"]`.
    pub fn sync_config_aliases(&mut self) {
        for (alias_key, target) in &self.settings.alias {
            let target = target.trim();
            if let Some(shell_cmd) = target.strip_prefix('!') {
                self.aliases
                    .entry("config".to_string())
                    .or_default()
                    .insert(
                        alias_key.clone(),
                        Command {
                            command: shell_cmd.trim().to_string(),
                            description: Some(format!("Git-style shell alias '{alias_key}'")),
                            platform: None,
                            aliases: vec![],
                            ..Default::default()
                        },
                    );
            }
        }
    }

    /// Loads the user configuration from `~/.config/fa/recipes.toml` and
    /// `~/.config/fa/recipes.d/*.toml`. Fails if any configuration error is found.
    pub fn load() -> anyhow::Result<(Self, String)> {
        let (config, source, errors) = Self::load_lenient()?;
        if let Some(err) = errors.into_iter().next() {
            anyhow::bail!("{err}");
        }
        Ok((config, source))
    }

    /// Loads the user configuration leniently: parses valid files, records
    /// any broken files or validation failures in `errors`, and continues
    /// loading other modular files.
    pub fn load_lenient() -> anyhow::Result<(Self, String, Vec<String>)> {
        let user_dir = Self::get_user_config_dir()
            .ok_or_else(|| anyhow::anyhow!("Cannot determine home directory (HOME not set)"))?;

        if !user_dir.exists() {
            Self::provision_example(&user_dir)?;
        }
        if let Some(state_dir) = crate::state::State::state_dir() {
            let _ = crate::schema::ensure_schema_file(&state_dir);
        }

        let mut config = Self::default();
        let mut primary_source = String::new();
        let mut errors = Vec::new();

        let xdg_path = user_dir.join("recipes.toml");
        if xdg_path.exists() {
            match fs::read_to_string(&xdg_path) {
                Ok(content) => match toml::from_str(&content) {
                    Ok(parsed) => {
                        config = parsed;
                        annotate_sources(&mut config, &xdg_path, &content);
                        primary_source = xdg_path.to_string_lossy().to_string();
                    }
                    Err(e) => {
                        errors.push(format!("Failed to parse {}: {e}", xdg_path.display()));
                    }
                },
                Err(e) => {
                    errors.push(format!("Failed to read {}: {e}", xdg_path.display()));
                }
            }
        }

        let mut loaded_modular_files = Vec::new();
        let xdg_d = user_dir.join("recipes.d");
        if xdg_d.is_dir() {
            Self::load_directory_lenient(&mut config, &xdg_d, &mut loaded_modular_files, &mut errors);
        }

        let global_config_path = user_dir.join("config.toml");
        if global_config_path.exists() {
            match fs::read_to_string(&global_config_path) {
                Ok(content) => match parse_global_config(&content) {
                    Ok(global) => {
                        config.settings = global;
                        config.sync_config_aliases();
                        annotate_sources(&mut config, &global_config_path, &content);
                    }
                    Err(e) => {
                        errors.push(format!("Failed to parse {}: {e}", global_config_path.display()));
                    }
                },
                Err(e) => {
                    errors.push(format!("Failed to read {}: {e}", global_config_path.display()));
                }
            }
        }

        let source_summary = if loaded_modular_files.is_empty() {
            primary_source
        } else {
            format!(
                "{} (+ {} modular file(s) in recipes.d/)",
                primary_source,
                loaded_modular_files.len()
            )
        };

        if let Err(e) = config.validate_template_paths() {
            errors.push(e.to_string());
        }
        if let Err(e) = config.validate_variables() {
            errors.push(e.to_string());
        }
        if let Err(e) = config.validate_steps() {
            errors.push(e.to_string());
        }
        if let Err(e) = config.validate_packs() {
            errors.push(e.to_string());
        }

        Ok((config, source_summary, errors))
    }

    /// Creates `~/.config/fa/` and writes the example configuration into it.
    /// The example is generated by `fa` itself (harmless `echo` alias), so it is
    /// auto-trusted: the user is never asked to confirm their own example file.
    fn provision_example(user_dir: &Path) -> anyhow::Result<()> {
        fs::create_dir_all(user_dir)
            .map_err(|e| anyhow::anyhow!("Failed to create {}: {e}", user_dir.display()))?;
        let example_path = user_dir.join("recipes.toml");
        fs::write(&example_path, EXAMPLE_CONFIG)
            .map_err(|e| anyhow::anyhow!("Failed to write {}: {e}", example_path.display()))?;
        let global_config_path = user_dir.join("config.toml");
        if !global_config_path.exists() {
            fs::write(&global_config_path, EXAMPLE_GLOBAL_CONFIG)
                .map_err(|e| anyhow::anyhow!("Failed to write {}: {e}", global_config_path.display()))?;
        }
        if let Some(state_dir) = crate::state::State::state_dir() {
            let _ = crate::schema::ensure_schema_file(&state_dir);
        }
        let mut state = crate::state::State::load();
        state.trust(&example_path.to_string_lossy());
        let _ = state.save();
        println!(
            "{}Initialized example config at {}{}",
            crate::colors::BOLD_GREEN,
            example_path.display(),
            crate::colors::RESET
        );
        Ok(())
    }

    fn load_directory_lenient(
        config: &mut Self,
        dir: &Path,
        loaded_files: &mut Vec<PathBuf>,
        errors: &mut Vec<String>,
    ) {
        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };

        let mut paths: Vec<PathBuf> = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("toml") {
                paths.push(path);
            }
        }

        paths.sort();

        for path in paths {
            let content = match fs::read_to_string(&path) {
                Ok(c) => c,
                Err(e) => {
                    errors.push(format!("Failed to read modular config {}: {e}", path.display()));
                    continue;
                }
            };
            let mut sub_config: Self = match toml::from_str(&content) {
                Ok(sc) => sc,
                Err(e) => {
                    errors.push(format!("Failed to parse modular config {}: {e}", path.display()));
                    continue;
                }
            };
            annotate_sources(&mut sub_config, &path, &content);

            for (recipe_name, recipe) in sub_config.recipes {
                config.recipes.insert(recipe_name, recipe);
            }

            for (section, commands) in sub_config.aliases {
                config
                    .aliases
                    .entry(section)
                    .or_default()
                    .extend(commands);
            }

            for (section, envs) in sub_config.alias_env {
                config
                    .alias_env
                    .entry(section)
                    .or_default()
                    .extend(envs);
            }

            for (section, envs) in sub_config.alias_env_force {
                config
                    .alias_env_force
                    .entry(section)
                    .or_default()
                    .extend(envs);
            }

            loaded_files.push(path);
        }
    }

    /// Normalizes a `from`/`template` spec to a path relative to
    /// `~/.config/fa/templates/`. Accepts the value with or without a single
    /// leading `templates/` prefix (it is stripped once when present, used
    /// as-is otherwise). Rejects absolute paths and `..` escapes with a clear
    /// error so misconfigurations never fail silently downstream.
    pub fn normalize_template_rel(spec: &str) -> anyhow::Result<String> {
        let rel = spec.strip_prefix("templates/").unwrap_or(spec);
        if rel.is_empty() {
            anyhow::bail!(
                "template path '{spec}' must stay inside ~/.config/fa/templates/"
            );
        }
        if Path::new(spec).is_absolute() || Path::new(rel).is_absolute() {
            anyhow::bail!(
                "template path '{spec}' must stay inside ~/.config/fa/templates/"
            );
        }
        if rel.split('/').any(|c| c == "..") {
            anyhow::bail!(
                "template path '{spec}' must stay inside ~/.config/fa/templates/"
            );
        }
        Ok(rel.to_string())
    }

    /// Validates every `from`/`template` reference in all recipes, failing fast
    /// with a clear error when a path would escape `~/.config/fa/templates/`.
    /// Resolution honors each recipe's `template_base` (see
    /// [`Self::resolve_rel`]): a `None` base resolves exactly like
    /// [`Self::normalize_template_rel`] (legacy byte-identical).
    fn validate_template_paths(&self) -> anyhow::Result<()> {
        for (recipe_key, recipe) in &self.recipes {
            if let Some(base) = &recipe.template_base {
                Self::normalize_template_rel(base).map_err(|e| {
                    anyhow::anyhow!(
                        "recipe '{recipe_key}': invalid template_base '{base}': {e}"
                    )
                })?;
            }
            if let Some(td) = &recipe.templates_dir
                && td.starts_with('/') {
                    anyhow::bail!(
                        "recipe '{recipe_key}': root paths starting with '/' are not allowed in templates_dir ('{td}'); use '~/' or paths relative to ~/.config/fa"
                    );
                }
            if let Some(pd) = &recipe.packs_dir
                && pd.starts_with('/') {
                    anyhow::bail!(
                        "recipe '{recipe_key}': root paths starting with '/' are not allowed in packs_dir ('{pd}'); use '~/' or paths relative to ~/.config/fa"
                    );
                }
            for (dest, spec) in &recipe.files {
                for value in [&spec.from, &spec.template].into_iter().flatten() {
                    Self::resolve_rel(recipe.template_base.as_deref(), value).map_err(|e| {
                        anyhow::anyhow!(
                            "recipe '{recipe_key}' file '{dest}': {e}"
                        )
                    })?;
                }
            }
        }
        Ok(())
    }

    /// Resolves a `from`/`template` spec against an optional recipe-level
    /// `template_base`, returning a path relative to
    /// `~/.config/fa/templates/`.
    ///
    /// - `None` base → identical to [`Self::normalize_template_rel`] (legacy).
    /// - `Some(base)` + prefixless spec → `base/spec` (both sides normalized).
    /// - `Some(base)` + spec starting with `templates/` → base is ignored
    ///   (legacy byte-identical escape hatch).
    /// - Absolute paths, `..` escapes and empty paths are rejected with a
    ///   `must stay inside ~/.config/fa/templates/` error.
    pub fn resolve_rel(base: Option<&str>, spec: &str) -> anyhow::Result<String> {
        // Legacy escape hatch: an explicit `templates/` prefix always ignores
        // the base so old configs keep resolving byte-identically.
        if spec.starts_with("templates/") {
            return Self::normalize_template_rel(spec);
        }
        let Some(base) = base else {
            return Self::normalize_template_rel(spec);
        };
        let base_rel = Self::normalize_template_rel(base)?;
        let spec_rel = Self::normalize_template_rel(spec)?;
        let joined = format!("{base_rel}/{spec_rel}");
        // Both halves are individually clean, so the join cannot escape; run
        // it through the normalizer once more for a single clear error path.
        Self::normalize_template_rel(&joined)
    }

    /// Returns the standard user configuration directory (~/.config/fa).
    pub fn get_user_config_dir() -> Option<PathBuf> {
        dirs_home_dir().map(|home| home.join(".config/fa"))
    }

    /// Returns the user templates directory (~/.config/fa/templates).
    pub fn get_user_templates_dir() -> Option<PathBuf> {
        Self::get_user_config_dir().map(|dir| dir.join("templates"))
    }

    /// Returns the single path whose trust grants access to all loaded config
    /// files ("one covers all"): the primary `recipes.toml` when present, or
    /// the `recipes.d/` directory when only modular files exist. Returns `None`
    /// when there is nothing to trust (empty catalog).
    pub fn trust_anchor() -> Option<PathBuf> {
        let user_dir = Self::get_user_config_dir()?;
        let primary = user_dir.join("recipes.toml");
        if primary.is_file() {
            return Some(primary);
        }
        let modular = user_dir.join("recipes.d");
        if modular.is_dir() {
            return Some(modular);
        }
        None
    }

    /// Resolves an input query (canonical recipe ID or any defined alias) to the canonical recipe key.
    pub fn resolve_recipe_key<'a>(&'a self, query: &'a str) -> Option<&'a String> {
        if self.recipes.contains_key(query) {
            return self.recipes.get_key_value(query).map(|(k, _)| k);
        }

        for (key, recipe) in &self.recipes {
            if recipe
                .aliases
                .iter()
                .any(|alias| alias.eq_ignore_ascii_case(query))
            {
                return Some(key);
            }
        }

        None
    }

    /// Returns the effective default variant for a recipe (first declared, or empty).
    pub fn default_variant(recipe: &Recipe) -> String {
        recipe.variants.first().cloned().unwrap_or_default()
    }

    /// Schema check for every `[variables]` entry in every recipe.
    pub fn validate_variables(&self) -> anyhow::Result<()> {
        for (recipe_key, recipe) in &self.recipes {
            for (var_name, var) in &recipe.variables {
                var.validate_schema(recipe_key, var_name)?;
            }
        }
        Ok(())
    }

    /// Validates that every `[[steps]]` has either `command` or `create`
    /// (not both, not neither).
    pub fn validate_steps(&self) -> anyhow::Result<()> {
        for (recipe_key, recipe) in &self.recipes {
            for (idx, step) in recipe.steps.iter().enumerate() {
                let has_command = step.command.is_some();
                let has_create = step.create.is_some();
                if !has_command && !has_create {
                    anyhow::bail!(
                        "recipe '{recipe_key}' step {idx}: must have either 'command' or 'create'"
                    );
                }
                if has_command && has_create {
                    anyhow::bail!(
                        "recipe '{recipe_key}' step {idx}: cannot have both 'command' and 'create'"
                    );
                }
                if let Some(create) = &step.create {
                    if create.from.starts_with('/') {
                        anyhow::bail!(
                            "recipe '{recipe_key}' step {idx}: root paths starting with '/' are not allowed in create.from ('{}'); use '~/' or paths relative to ~/.config/fa",
                            create.from
                        );
                    }
                    if create.to.starts_with('/') {
                        anyhow::bail!(
                            "recipe '{recipe_key}' step {idx}: create.to path ('{}') must be relative to project and cannot start with '/'",
                            create.to
                        );
                    }
                }
            }
        }
        Ok(())
    }

    /// Validates packs configuration:
    /// 1. `settings.packs.default_behavior` must be one of: 'list', 'default', 'error'.
    /// 2. If any recipe defines `default_pack`, `default_behavior` must be set to 'default'.
    ///    Otherwise, it errors instructing the user to change `default_behavior = "default"` in `~/.config/fa/config.toml`.
    pub fn validate_packs(&self) -> anyhow::Result<()> {
        let behavior = self.settings.packs.default_behavior.as_str();
        if !matches!(behavior, "list" | "default" | "error") {
            anyhow::bail!(
                "invalid packs.default_behavior '{behavior}' in config.toml; expected one of: 'list', 'default', 'error'"
            );
        }

        for (recipe_key, recipe) in &self.recipes {
            if let Some(ref dp) = recipe.default_pack
                && behavior != "default" {
                    anyhow::bail!(
                        "recipe '{recipe_key}': 'default_pack' is set to '{dp}', but global packs behavior is '{behavior}'. Change default_behavior = \"default\" in ~/.config/fa/config.toml to enable default packs in recipes"
                    );
                }
        }
        Ok(())
    }

    /// Finds a pack definition from either:
    /// 1. The recipe's inline `[recipes.<name>.packs.<pack_name>]`
    /// 2. A `<pack_name>.toml` file in `packs_dir`
    /// 3. A multi-pack TOML file in `packs_dir` (e.g. `packs.toml` or any `.toml` containing `[packs.<name>]` or `[<name>]`)
    pub fn find_pack(recipe: Option<&Recipe>, packs_dir: &str, pack_name: &str) -> anyhow::Result<Pack> {
        if let Some(r) = recipe {
            if let Some(def) = r.packs.get(pack_name) {
                return Ok(Pack {
                    name: def.name.clone().unwrap_or_else(|| pack_name.to_string()),
                    description: def.description.clone(),
                    components: def.components.clone(),
                });
            }
            for (key, def) in &r.packs {
                if def.name.as_deref() == Some(pack_name) {
                    return Ok(Pack {
                        name: key.clone(),
                        description: def.description.clone(),
                        components: def.components.clone(),
                    });
                }
            }
        }

        if packs_dir.starts_with('/') {
            anyhow::bail!("Root paths starting with '/' are not allowed; use '~/' or paths relative to ~/.config/fa");
        }
        let pack_dir = if let Some(rest) = packs_dir.strip_prefix("~/") {
            let home = dirs_home_dir()
                .ok_or_else(|| anyhow::anyhow!("Cannot determine home directory (HOME not set)"))?;
            home.join(rest)
        } else {
            let Some(user_dir) = Self::get_user_config_dir() else {
                anyhow::bail!("Cannot determine home directory (HOME not set)");
            };
            user_dir.join(packs_dir)
        };

        let pack_path = pack_dir.join(format!("{pack_name}.toml"));
        if pack_path.is_file()
            && let Ok(content) = fs::read_to_string(&pack_path)
        {
            let stem = pack_path.file_stem().and_then(|s| s.to_str()).unwrap_or(pack_name);
            let parsed = parse_packs_from_toml(&content, stem);
            if let Some(found) = parsed.into_iter().find(|p| p.name == pack_name) {
                return Ok(found);
            }
        }

        if let Ok(entries) = fs::read_dir(&pack_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file()
                    && path.extension().and_then(|s| s.to_str()) == Some("toml")
                    && let Ok(content) = fs::read_to_string(&path)
                {
                    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                    let parsed = parse_packs_from_toml(&content, stem);
                    if let Some(found) = parsed.into_iter().find(|p| p.name == pack_name) {
                        return Ok(found);
                    }
                }
            }
        }

        anyhow::bail!("Pack '{pack_name}' not found at {}", pack_path.display());
    }

    /// Loads a pack definition from `~/.config/fa/<packs_dir>/<name>.toml`
    /// or from a custom user directory starting with `~/`.
    #[allow(dead_code)]
    pub fn load_pack(packs_dir: &str, pack_name: &str) -> anyhow::Result<Pack> {
        Self::find_pack(None, packs_dir, pack_name)
    }

    /// Lists all packs for a recipe, combining inline packs with packs found in `packs_dir`.
    pub fn list_recipe_packs(recipe: &Recipe) -> Vec<Pack> {
        let mut map: BTreeMap<String, Pack> = BTreeMap::new();

        for (key, def) in &recipe.packs {
            let name = def.name.clone().unwrap_or_else(|| key.clone());
            map.insert(name.clone(), Pack {
                name,
                description: def.description.clone(),
                components: def.components.clone(),
            });
        }

        let packs_dir = recipe.packs_dir.as_deref().unwrap_or("packs");
        for pack in Self::list_packs(packs_dir) {
            map.entry(pack.name.clone()).or_insert(pack);
        }

        map.into_values().collect()
    }

    /// Returns the effective templates directory for a recipe.
    /// Falls back to the default `~/.config/fa/templates/`.
    pub fn resolve_templates_dir(recipe: &Recipe) -> String {
        recipe.templates_dir.clone().unwrap_or_else(|| "templates".to_string())
    }

    /// Resolves an input query to its full command definition, returning
    /// `(section, command_key, &Command)`.
    ///
    /// Supports:
    /// - Flat aliases in standard sections (e.g. `[aliases.git]`, `[aliases.general]`)
    /// - Namespaced subcommands (e.g. `skills ls` or `:skills ls` from `[aliases.":skills"]`)
    /// - Namespaced root commands (e.g. `skills` or `:skills` when `[aliases.":skills"]` contains `skills`)
    /// - Preserves strict isolation: subcommands inside `:namespace` do NOT leak to flat queries.
    pub fn resolve_command(&self, query: &str) -> Option<(String, String, &Command)> {
        let trimmed = query.trim();

        if let Some((ns_part, subcmd_part)) = trimmed.split_once(' ') {
            let ns_key = if ns_part.starts_with(':') {
                ns_part.to_string()
            } else {
                format!(":{ns_part}")
            };
            if let Some(commands) = self.aliases.get(&ns_key)
                && let Some((key, command)) = find_command(commands, subcmd_part)
            {
                return Some((ns_key, key.clone(), command));
            }
            return None;
        }

        for (section, commands) in &self.aliases {
            if section.starts_with(':') {
                continue; // Isolated: commands in namespaced sections do not leak to flat queries
            }
            if let Some((key, command)) = find_command(commands, trimmed) {
                return Some((section.clone(), key.clone(), command));
            }
        }

        let ns_key = if trimmed.starts_with(':') {
            trimmed.to_string()
        } else {
            format!(":{trimmed}")
        };
        let raw_ns = ns_key.strip_prefix(':').unwrap_or(&ns_key);
        if let Some(commands) = self.aliases.get(&ns_key)
            && let Some((key, command)) = find_command(commands, raw_ns)
        {
            return Some((ns_key, key.clone(), command));
        }

        None
    }

    /// Lists all packs found in `packs_dir` (resolving `~/` or `~/.config/fa/<packs_dir>`).
    pub fn list_packs(packs_dir: &str) -> Vec<Pack> {
        let pack_dir = if let Some(rest) = packs_dir.strip_prefix("~/") {
            dirs_home_dir().map(|h| h.join(rest))
        } else {
            Self::get_user_config_dir().map(|u| u.join(packs_dir))
        };
        let Some(dir) = pack_dir else {
            return Vec::new();
        };
        let entries = match fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => return Vec::new(),
        };
        let mut map: BTreeMap<String, Pack> = BTreeMap::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file()
                && path.extension().and_then(|s| s.to_str()) == Some("toml")
                && let Ok(content) = fs::read_to_string(&path)
            {
                let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                for pack in parse_packs_from_toml(&content, stem) {
                    map.entry(pack.name.clone()).or_insert(pack);
                }
            }
        }
        map.into_values().collect()
    }

    /// Lists all components found in the recipe's `templates_dir`.
    pub fn list_components(recipe: &Recipe) -> Vec<String> {
        let templates_dir = Self::resolve_templates_dir(recipe);
        let comp_dir = if let Some(rest) = templates_dir.strip_prefix("~/") {
            dirs_home_dir().map(|h| h.join(rest))
        } else {
            Self::get_user_config_dir().map(|u| u.join(&templates_dir))
        };
        let Some(dir) = comp_dir else {
            return Vec::new();
        };
        let entries = match fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => return Vec::new(),
        };
        let mut components = Vec::new();
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with('.') {
                components.push(name);
            }
        }
        components.sort();
        components
    }

    /// Returns (section, command_key, &Command) for every command in the
    /// alias catalog.
    pub fn all_commands(&self) -> Vec<(&str, &String, &Command)> {
        let mut out = Vec::new();
        for (section, commands) in &self.aliases {
            for (command_key, command) in commands {
                out.push((section.as_str(), command_key, command));
            }
        }
        out
    }
}

impl Recipe {
    /// True if the recipe defines pack/component workflows (has `packs_dir`, `templates_dir`,
    /// `default_pack`, inline `packs`, or steps with `create`).
    pub fn is_pack_recipe(&self) -> bool {
        self.packs_dir.is_some()
            || self.templates_dir.is_some()
            || self.default_pack.is_some()
            || !self.packs.is_empty()
            || self.steps.iter().any(|s| s.create.is_some())
    }
}

/// Helper to resolve the user's home directory from environment.
pub fn dirs_home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// Finds a command by its canonical key or any alias (case-insensitive).
pub(crate) fn find_command<'a>(
    commands: &'a BTreeMap<String, Command>,
    query: &str,
) -> Option<(&'a String, &'a Command)> {
    if let Some(key) = commands.keys().find(|k| k.eq_ignore_ascii_case(query)) {
        return commands.get_key_value(key);
    }
    commands.iter().find_map(|(k, cmd)| {
        cmd.aliases
            .iter()
            .any(|alias| alias.eq_ignore_ascii_case(query))
            .then_some((k, cmd))
    })
}

fn split_toml_path(s: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut in_quotes = None;

    for ch in s.chars() {
        if let Some(q) = in_quotes {
            if ch == q {
                in_quotes = None;
            } else {
                current.push(ch);
            }
        } else if ch == '"' || ch == '\'' {
            in_quotes = Some(ch);
        } else if ch == '.' {
            let trimmed = current.trim();
            if !trimmed.is_empty() {
                parts.push(trimmed.to_string());
            }
            current.clear();
        } else {
            current.push(ch);
        }
    }
    let trimmed = current.trim();
    if !trimmed.is_empty() {
        parts.push(trimmed.to_string());
    }
    parts
}

fn extract_toml_key(line: &str) -> Option<String> {
    let mut in_quotes = None;
    let mut key_buf = String::new();

    for ch in line.chars() {
        if let Some(q) = in_quotes {
            if ch == q {
                in_quotes = None;
            } else {
                key_buf.push(ch);
            }
        } else if ch == '"' || ch == '\'' {
            in_quotes = Some(ch);
        } else if ch == '#' {
            break;
        } else if ch == '=' {
            let trimmed = key_buf.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
            return None;
        } else {
            key_buf.push(ch);
        }
    }
    None
}

/// Annotates each recipe and command with its source file path and line number.
pub fn annotate_sources(config: &mut Config, path: &Path, content: &str) {
    #[derive(Debug, Clone)]
    enum Context {
        None,
        Recipe,
        AliasSection(String),
        GlobalAlias,
    }

    let mut context = Context::None;

    for (idx, line) in content.lines().enumerate() {
        let line_num = idx + 1;
        let trimmed = line.trim();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            let inner = trimmed
                .trim_start_matches('[')
                .trim_end_matches(']')
                .trim();

            if let Some(rest) = inner.strip_prefix("recipes.") {
                let parts = split_toml_path(rest);
                if let Some(recipe_key) = parts.first() {
                    context = Context::Recipe;
                    if parts.len() == 1
                        && let Some(recipe) = config.recipes.get_mut(recipe_key)
                    {
                        recipe.source_file = Some(path.to_path_buf());
                        recipe.source_line = Some(line_num);
                    }
                }
            } else if let Some(rest) = inner
                .strip_prefix("aliases.")
                .or_else(|| inner.strip_prefix("alias."))
            {
                let parts = split_toml_path(rest);
                if parts.len() == 1 {
                    context = Context::AliasSection(parts[0].clone());
                } else if parts.len() == 2 {
                    let sec = &parts[0];
                    let cmd_name = &parts[1];
                    context = Context::AliasSection(sec.clone());
                    if let Some(section) = config.aliases.get_mut(sec)
                        && let Some(cmd) = section.get_mut(cmd_name)
                    {
                        cmd.source_file = Some(path.to_path_buf());
                        cmd.source_line = Some(line_num);
                    }
                } else {
                    context = Context::None;
                }
            } else if inner == "alias" {
                context = Context::GlobalAlias;
            } else {
                context = Context::None;
            }
            continue;
        }

        match &context {
            Context::AliasSection(sec) => {
                if let Some(key) = extract_toml_key(trimmed)
                    && let Some(section) = config.aliases.get_mut(sec)
                    && let Some(cmd) = section.get_mut(&key)
                {
                    cmd.source_file = Some(path.to_path_buf());
                    cmd.source_line = Some(line_num);
                }
            }
            Context::GlobalAlias => {
                if let Some(key) = extract_toml_key(trimmed)
                    && let Some(section) = config.aliases.get_mut("config")
                    && let Some(cmd) = section.get_mut(&key)
                {
                    cmd.source_file = Some(path.to_path_buf());
                    cmd.source_line = Some(line_num);
                }
            }
            _ => {}
        }
    }
}

/// Parses a bash-style environment assignment string (e.g. `'export FOO="bar" && export BAZ=qux'` or `'KEY=val'`)
/// into a key-value map.
pub fn parse_env_string(input: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        while i < len {
            if chars[i].is_whitespace() || chars[i] == ';' {
                i += 1;
            } else if chars[i] == '&' && i + 1 < len && chars[i + 1] == '&' {
                i += 2;
            } else {
                break;
            }
        }
        if i >= len {
            break;
        }

        if i + 6 <= len
            && chars[i..i + 6] == ['e', 'x', 'p', 'o', 'r', 't']
            && (i + 6 == len || chars[i + 6].is_whitespace())
        {
            i += 6;
            while i < len && chars[i].is_whitespace() {
                i += 1;
            }
        }
        if i >= len {
            break;
        }

        let key_start = i;
        while i < len && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
            i += 1;
        }
        let key: String = chars[key_start..i].iter().collect();

        if key.is_empty() {
            i += 1;
            continue;
        }

        while i < len && (chars[i] == ' ' || chars[i] == '\t') {
            i += 1;
        }
        if i >= len || chars[i] != '=' {
            continue;
        }
        i += 1; // skip '='

        while i < len && (chars[i] == ' ' || chars[i] == '\t') {
            i += 1;
        }
        if i >= len {
            map.insert(key, String::new());
            break;
        }

        let mut val = String::new();
        if chars[i] == '"' {
            i += 1; // skip opening quote
            while i < len {
                if chars[i] == '\\' && i + 1 < len && (chars[i + 1] == '"' || chars[i + 1] == '\\') {
                    val.push(chars[i + 1]);
                    i += 2;
                } else if chars[i] == '"' {
                    i += 1; // skip closing quote
                    break;
                } else {
                    val.push(chars[i]);
                    i += 1;
                }
            }
        } else if chars[i] == '\'' {
            i += 1; // skip opening quote
            while i < len {
                if chars[i] == '\'' {
                    i += 1; // skip closing quote
                    break;
                } else {
                    val.push(chars[i]);
                    i += 1;
                }
            }
        } else {
            while i < len {
                if chars[i].is_whitespace() || chars[i] == ';' {
                    break;
                }
                if chars[i] == '&' && i + 1 < len && chars[i + 1] == '&' {
                    break;
                }
                val.push(chars[i]);
                i += 1;
            }
        }

        map.insert(key, val);
    }

    map
}

/// Expands `$VAR`, `${VAR}`, and leading `~` references in an environment variable value using the current process environment.
pub fn expand_env_value(val: &str) -> String {
    let mut input = val.to_string();
    if input == "~" {
        if let Ok(home) = std::env::var("HOME") {
            return home;
        }
    } else if let Some(rest) = input.strip_prefix("~/")
        && let Ok(home) = std::env::var("HOME")
    {
        input = format!("{home}/{rest}");
    }

    let mut result = String::new();
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '$' && i + 1 < chars.len() {
            if chars[i + 1] == '{' {
                // Braced variable: ${VAR} or ${VAR:-default}
                if let Some(close_idx) = chars[i + 2..].iter().position(|&c| c == '}') {
                    let full_close = i + 2 + close_idx;
                    let inner: String = chars[i + 2..full_close].iter().collect();
                    if let Some((var_name, default_val)) = inner.split_once(":-") {
                        let val = std::env::var(var_name.trim())
                            .ok()
                            .filter(|s| !s.is_empty())
                            .unwrap_or_else(|| default_val.to_string());
                        result.push_str(&val);
                    } else if let Some((var_name, default_val)) = inner.split_once(':') {
                        let val = std::env::var(var_name.trim())
                            .ok()
                            .filter(|s| !s.is_empty())
                            .unwrap_or_else(|| default_val.to_string());
                        result.push_str(&val);
                    } else {
                        let val = std::env::var(inner.trim()).unwrap_or_default();
                        result.push_str(&val);
                    }
                    i = full_close + 1;
                    continue;
                }
            } else if chars[i + 1].is_ascii_alphabetic() || chars[i + 1] == '_' {
                // Unbraced variable: $VAR
                let mut end = i + 1;
                while end < chars.len() && (chars[end].is_ascii_alphanumeric() || chars[end] == '_') {
                    end += 1;
                }
                let var_name: String = chars[i + 1..end].iter().collect();
                let val = std::env::var(&var_name).unwrap_or_default();
                result.push_str(&val);
                i = end;
                continue;
            }
        }
        result.push(chars[i]);
        i += 1;
    }

    result
}

/// Computes the effective environment variables for an alias execution honoring:
/// 1. Fallback vars (`_env` and `env`): only applied if not set in the current process environment.
/// 2. Forced vars (`_env_force` and `env_force`): always applied, overriding the current process environment.
///
/// Variable values are expanded via [`expand_env_value`].
pub fn resolve_alias_env(config: &Config, section: &str, cmd: &Command) -> Vec<(String, String)> {
    let mut resolved: BTreeMap<String, String> = BTreeMap::new();

    if let Some(ns_env) = config.alias_env.get(section) {
        for (k, v) in ns_env {
            if std::env::var(k).is_err() {
                resolved.insert(k.clone(), v.clone());
            }
        }
    }

    for (k, v) in &cmd.env {
        if std::env::var(k).is_err() {
            resolved.insert(k.clone(), v.clone());
        }
    }

    if let Some(ns_force) = config.alias_env_force.get(section) {
        for (k, v) in ns_force {
            resolved.insert(k.clone(), v.clone());
        }
    }

    for (k, v) in &cmd.env_force {
        resolved.insert(k.clone(), v.clone());
    }

    resolved
        .into_iter()
        .map(|(k, v)| (k, expand_env_value(&v)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal sample catalog used to exercise resolution logic without
    /// depending on any user-provided or example configuration.
    const SAMPLE_CONFIG: &str = r#"
[recipes.demo]
name = "Demo"
description = "A demo recipe"
language = "web · typescript"
variants = ["pnpm", "bun"]

[aliases.deploy]
deploy = { command = "node --run build", description = "Build and deploy", aliases = ["fpages", "cfp"] }
check = { command = "node --run check", description = "Run checks" }
"#;

    #[test]
    fn example_config_should_parse_and_contain_example_alias() {
        println!("\n🔍 [TEST] Example Config TOML Parsing");
        println!("   Explanation: Verifies the first-run example catalog parses successfully and contains the 'example' alias.");

        let config: Result<Config, _> = toml::from_str(EXAMPLE_CONFIG);
        assert!(config.is_ok(), "Example TOML should parse without errors");

        let cfg = config.unwrap();
        println!("   ✓ Valid TOML structure. Total recipes loaded: {}", cfg.recipes.len());

        assert!(cfg.recipes.contains_key("example"), "Must include 'example' recipe");
        println!("   ✓ Recipe 'example' verified in catalog.\n");

        assert!(
            cfg.aliases["demo"].contains_key("hello"),
            "Example config should declare a 'hello' alias"
        );
        println!("   ✓ Alias 'hello' verified in catalog.\n");
    }

    #[test]
    fn recipe_alias_resolution_should_match_canonical_and_alias_keys() {
        let config: Config = toml::from_str(SAMPLE_CONFIG).expect("Should parse sample config");

        assert_eq!(config.resolve_recipe_key("demo"), Some(&"demo".to_string()));
        assert_eq!(config.resolve_recipe_key("nonexistent"), None);
    }

    #[test]
    fn command_resolution_should_match_canonical_and_alias_keys() {
        let config: Config = toml::from_str(SAMPLE_CONFIG).expect("Should parse sample config");

        assert_eq!(
            config.resolve_command("deploy").map(|(_, key, _)| key),
            Some("deploy".to_string())
        );
        assert_eq!(
            config.resolve_command("fpages").map(|(_, key, _)| key),
            Some("deploy".to_string())
        );
        assert!(config.resolve_command("nonexistent").is_none());
    }

    #[test]
    fn alias_sections_should_resolve_across_sections() {
        let config: Config = toml::from_str(
            r#"
[recipes.demo]
name = "Demo"
description = "A demo recipe"

[aliases.git]
status = { command = "git status", description = "Repo state" }
gco = { command = "git checkout {{branch}}", description = "Switch branch", aliases = ["co"] }

[aliases.sistema]
free = { command = "free -h", description = "Free memory" }
"#,
        )
        .expect("Should parse config with alias sections");

        assert_eq!(
            config.resolve_command("gco").map(|(section, key, _)| (section, key)),
            Some(("git".to_string(), "gco".to_string()))
        );
        assert_eq!(
            config.resolve_command("co").map(|(section, key, _)| (section, key)),
            Some(("git".to_string(), "gco".to_string()))
        );
        assert_eq!(
            config.resolve_command("free").map(|(section, key, _)| (section, key)),
            Some(("sistema".to_string(), "free".to_string()))
        );
        assert_eq!(
            config.resolve_command("status").map(|(section, key, _)| (section, key)),
            Some(("git".to_string(), "status".to_string()))
        );
        assert!(config.resolve_command("ghost").is_none());

        let grouped = config.all_commands();
        assert_eq!(grouped.len(), 3, "Both alias sections are listed");
    }

    #[test]
    fn recipe_names_and_aliases_should_not_have_exact_duplicates() {
        use std::collections::HashSet;

        let config: Config = toml::from_str(SAMPLE_CONFIG).expect("Should parse sample config");
        let mut seen_keys = HashSet::new();

        for (key, recipe) in &config.recipes {
            assert!(
                seen_keys.insert(key.to_lowercase()),
                "Duplicate recipe name found: '{key}'"
            );
            for alias in &recipe.aliases {
                assert!(
                    seen_keys.insert(alias.to_lowercase()),
                    "Duplicate recipe alias or collision with recipe name found: '{alias}'"
                );
            }
        }
    }

    #[test]
    fn variant_resolution_should_fall_back_to_first_declared() {
        let config: Config = toml::from_str(SAMPLE_CONFIG).expect("Should parse sample config");
        let demo = config.recipes.get("demo").expect("demo recipe should exist");
        assert_eq!(Config::default_variant(demo), "pnpm");
    }

    #[test]
    fn invalid_toml_should_fail_gracefully() {
        let broken_toml = "this is not = [valid toml content {{";
        let parsed: Result<Config, _> = toml::from_str(broken_toml);
        assert!(parsed.is_err(), "Invalid TOML must return deserialization error");
    }

    #[test]
    fn template_path_should_accept_with_or_without_prefix() {
        println!("\n🔍 [TEST] Template path — with/without `templates/` prefix resolves equally");
        let with = Config::normalize_template_rel("templates/my-recipe/file.txt")
            .expect("prefixed path must normalize");
        let without = Config::normalize_template_rel("my-recipe/file.txt")
            .expect("prefixless path must normalize");
        assert_eq!(with, "my-recipe/file.txt");
        assert_eq!(without, "my-recipe/file.txt");
        // Only one leading prefix is stripped: a nested `templates/` dir stays.
        let nested = Config::normalize_template_rel("templates/templates/x")
            .expect("nested templates dir must normalize");
        assert_eq!(nested, "templates/x");
        println!("   ✓ Prefixed and prefixless paths normalize to the same rel.\n");
    }

    #[test]
    fn template_path_should_reject_escape_with_clear_error() {
        println!("\n🔍 [TEST] Template path — traversal/absolute rejected with clear error");
        for bad in ["../evil.txt", "templates/../evil.txt", "a/../../b", "/abs/path.txt", "templates/"] {
            let err = Config::normalize_template_rel(bad)
                .err()
                .unwrap_or_else(|| panic!("'{bad}' must be rejected"));
            let msg = err.to_string();
            assert!(
                msg.contains("must stay inside ~/.config/fa/templates/"),
                "Error must name the templates dir, got: '{msg}'"
            );
        }
        println!("   ✓ Out-of-range paths rejected with clear error (not silent).\n");
    }

    #[test]
    fn template_base_should_default_to_none_for_legacy_compat() {
        println!("\n🔍 [TEST] template_base — defaults to None (legacy byte-identical)");
        let config: Config = toml::from_str(
            r#"
[recipes.demo]
name = "Demo"
description = "A demo recipe"
"#,
        )
        .expect("Should parse recipe without template_base");
        let demo = config.recipes.get("demo").expect("demo must exist");
        assert!(demo.template_base.is_none(), "legacy recipes default to None");
        println!("   ✓ Missing template_base defaults to None.\n");
    }

    #[test]
    fn resolve_rel_should_keep_legacy_identical_when_base_is_none() {
        println!("\n🔍 [TEST] resolve_rel — base None keeps legacy identical");
        let with = Config::resolve_rel(None, "templates/my-recipe/file.txt")
            .expect("prefixed must resolve");
        let bare = Config::resolve_rel(None, "my-recipe/file.txt")
            .expect("prefixless must resolve");
        assert_eq!(with, "my-recipe/file.txt");
        assert_eq!(bare, "my-recipe/file.txt");
        assert_eq!(with, bare, "both forms must resolve equally with None base");
        println!("   ✓ Base None resolves exactly like legacy normalize.\n");
    }

    #[test]
    fn resolve_rel_should_join_base_and_spec() {
        println!("\n🔍 [TEST] resolve_rel — base + from join");
        let rel = Config::resolve_rel(Some("rust-stack"), ".gitignore")
            .expect("base + spec must join");
        assert_eq!(rel, "rust-stack/.gitignore");
        let nested = Config::resolve_rel(Some("rust-stack"), "config/.editorconfig")
            .expect("nested spec must join");
        assert_eq!(nested, "rust-stack/config/.editorconfig");
        println!("   ✓ Joined rel verified.\n");
    }

    #[test]
    fn resolve_rel_should_ignore_base_when_spec_has_templates_prefix() {
        println!("\n🔍 [TEST] resolve_rel — `templates/` spec ignores base (legacy byte-identical)");
        let with_base =
            Config::resolve_rel(Some("rust-stack"), "templates/other/file.txt")
                .expect("prefixed spec must resolve");
        let without_base = Config::resolve_rel(None, "templates/other/file.txt")
            .expect("legacy must resolve");
        assert_eq!(with_base, "other/file.txt");
        assert_eq!(with_base, without_base, "prefixed spec must ignore base entirely");
        println!("   ✓ Prefixed spec ignores base.\n");
    }

    #[test]
    fn resolve_rel_should_reject_escapes_with_clear_error() {
        println!("\n🔍 [TEST] resolve_rel — escapes/absolutes rejected with clear error");
        for (base, spec) in [
            (Some("rust-stack"), "../evil.txt"),
            (Some("../evil"), "file.txt"),
            (Some("rust-stack"), "/abs/x.txt"),
            (Some("rust-stack"), ""),
            (Some("rust-stack"), "templates/../evil.txt"),
            (None, "../evil.txt"),
        ] {
            let err = Config::resolve_rel(base, spec)
                .err()
                .unwrap_or_else(|| panic!("base={base:?} spec='{spec}' must be rejected"));
            let msg = err.to_string();
            assert!(
                msg.contains("must stay inside ~/.config/fa/templates/"),
                "Error must name the templates dir, got: '{msg}'"
            );
        }
        println!("   ✓ Escapes rejected clearly.\n");
    }

    #[test]
    fn var_schema_should_reject_unknown_type() {
        let cfg: Config = toml::from_str(
            r#"
[recipes.demo]
name = "Demo"
description = "d"
[recipes.demo.variables]
port = { prompt = "Port", default = "3000", type = "date" }
"#,
        )
        .expect("shape must parse; type checked at validation");
        let err = cfg.validate_variables().expect_err("unknown type must fail");
        let msg = err.to_string();
        assert!(msg.contains("demo") && msg.contains("port") && msg.contains("date"),
            "error must name recipe + variable + bad type, got: '{msg}'");
    }

    #[test]
    fn var_schema_should_reject_unknown_keys() {
        let parsed: Result<Config, _> = toml::from_str(
            r#"
[recipes.demo]
name = "Demo"
description = "d"
[recipes.demo.variables]
port = { prompt = "Port", default = "3000", bogus_key = "x" }
"#,
        );
        let err = parsed.expect_err("unknown variable key must fail at load");
        assert!(err.to_string().contains("bogus_key"),
            "error must name the unknown key, got: '{err}'");
    }

    #[test]
    fn var_schema_should_reject_empty_choices() {
        let cfg: Config = toml::from_str(
            r#"
[recipes.demo]
name = "Demo"
description = "d"
[recipes.demo.variables]
pm = { prompt = "PM", choices = [] }
"#,
        )
        .expect("shape must parse");
        let err = cfg.validate_variables().expect_err("empty choices must fail");
        assert!(err.to_string().contains("must not be empty"), "got: '{err}'");
    }

    #[test]
    fn var_schema_should_reject_pattern_with_non_string_type() {
        let cfg: Config = toml::from_str(
            r#"
[recipes.demo]
name = "Demo"
description = "d"
[recipes.demo.variables]
port = { prompt = "Port", type = "integer", pattern = "^[0-9]+$" }
"#,
        )
        .expect("shape must parse");
        let err = cfg.validate_variables().expect_err("pattern + integer must fail");
        assert!(err.to_string().contains("requires type"), "got: '{err}'");
    }

    #[test]
    fn var_schema_should_reject_invalid_regex() {
        let cfg: Config = toml::from_str(
            r#"
[recipes.demo]
name = "Demo"
description = "d"
[recipes.demo.variables]
slug = { prompt = "Slug", pattern = "([a-" }
"#,
        )
        .expect("shape must parse");
        let err = cfg.validate_variables().expect_err("invalid regex must fail");
        assert!(err.to_string().contains("invalid pattern"), "got: '{err}'");
    }

    #[test]
    fn var_schema_should_accept_valid_optional_keys() {
        let cfg: Config = toml::from_str(
            r#"
[recipes.demo]
name = "Demo"
description = "d"
[recipes.demo.variables]
port = { prompt = "Port", default = "3000", type = "integer", choices = ["3000", "8080"] }
slug = { prompt = "Slug", type = "string", pattern = "^[a-z0-9-]+$", required = true }
"#,
        )
        .expect("valid typed variables must parse");
        cfg.validate_variables().expect("valid typed variables must validate");
        assert!(cfg.recipes["demo"].variables["port"].is_typed());
    }

    #[test]
    fn var_plain_prompt_without_typing_should_keep_working() {
        let cfg: Config = toml::from_str(
            r#"
[recipes.demo]
name = "Demo"
description = "d"
[recipes.demo.variables]
name = { prompt = "Project name", default = "app" }
"#,
        )
        .expect("plain variable must parse");
        cfg.validate_variables().expect("plain variable must validate");
        let v = &cfg.recipes["demo"].variables["name"];
        assert!(!v.is_typed(), "untyped variable must report is_typed() == false");
        assert!(v.validate_value("name", "anything at all!@#").is_ok());
        assert!(v.validate_value("name", "").is_ok(), "empty stays allowed when not required");
    }

    #[test]
    fn var_value_should_reject_bad_type_choice_pattern_and_required_empty() {
        let int_var = Variable {
            prompt: "Port".into(), default: Some("3000".into()),
            var_type: Some("integer".into()), choices: None, pattern: None, required: None,
        };
        assert!(int_var.validate_value("port", "abc").is_err(), "non-integer must fail");
        assert!(int_var.validate_value("port", "8080").is_ok());

        let choice_var = Variable {
            prompt: "PM".into(), default: None,
            var_type: None, choices: Some(vec!["pnpm".into(), "bun".into()]),
            pattern: None, required: None,
        };
        assert!(choice_var.validate_value("pm", "npm").is_err(), "off-list choice must fail");
        assert!(choice_var.validate_value("pm", "pnpm").is_ok());

        let pattern_var = Variable {
            prompt: "Slug".into(), default: None,
            var_type: Some("string".into()), choices: None,
            pattern: Some("^[a-z0-9-]+$".into()), required: None,
        };
        assert!(pattern_var.validate_value("slug", "Bad Name!").is_err(), "pattern mismatch must fail");
        assert!(pattern_var.validate_value("slug", "my-app-1").is_ok());

        let req_var = Variable {
            prompt: "Token".into(), default: None,
            var_type: None, choices: None, pattern: None, required: Some(true),
        };
        assert!(req_var.validate_value("token", "").is_err(), "empty required must fail");
        assert!(req_var.validate_value("token", "  ").is_err(), "blank required must fail");
        assert!(req_var.validate_value("token", "x").is_ok());
    }

    #[test]
    fn validate_packs_should_reject_default_pack_when_behavior_is_not_default() {
        let toml_content = r#"
[recipes.wc-lib]
name = "wc-lib"
description = "Web Components"
default_pack = "toggle-theme"
"#;
        let cfg: Config = toml::from_str(toml_content).unwrap();
        // default_behavior is "list" by default
        assert_eq!(cfg.settings.packs.default_behavior, "list");
        let res = cfg.validate_packs();
        assert!(res.is_err(), "Must reject default_pack when default_behavior is 'list'");
        let err = res.unwrap_err().to_string();
        assert!(
            err.contains("Change default_behavior = \"default\" in ~/.config/fa/config.toml"),
            "Error must instruct to change config.toml, got: {err}"
        );
    }

    #[test]
    fn validate_packs_should_allow_default_pack_when_behavior_is_default() {
        let toml_content = r#"
[recipes.wc-lib]
name = "wc-lib"
description = "Web Components"
default_pack = "toggle-theme"
"#;
        let mut cfg: Config = toml::from_str(toml_content).unwrap();
        cfg.settings.packs.default_behavior = "default".to_string();
        assert!(cfg.validate_packs().is_ok(), "Must allow default_pack when default_behavior is 'default'");
    }

    #[test]
    fn validate_packs_should_reject_invalid_behavior() {
        let mut cfg = Config::default();
        cfg.settings.packs.default_behavior = "unknown_mode".to_string();
        let res = cfg.validate_packs();
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("expected one of: 'list', 'default', 'error'"));
    }

    #[test]
    fn test_recipe_default_pack_rename_and_alias() {
        let toml_new = r#"
[recipes.test1]
name = "Test 1"
description = "d"
default_pack = "my-pack"
"#;
        let cfg: Config = toml::from_str(toml_new).unwrap();
        assert_eq!(cfg.recipes["test1"].default_pack.as_deref(), Some("my-pack"));

        let toml_legacy = r#"
[recipes.test2]
name = "Test 2"
description = "d"
default = "my-pack"
"#;
        let cfg: Config = toml::from_str(toml_legacy).unwrap();
        assert_eq!(cfg.recipes["test2"].default_pack.as_deref(), Some("my-pack"));
    }

    #[test]
    fn test_global_config_loads_all_three_modes() {
        let cfg_default = GlobalConfig::default();
        assert_eq!(cfg_default.packs.default_behavior, "list");

        let toml_list = "[packs]\ndefault_behavior = \"list\"";
        let cfg_list: GlobalConfig = toml::from_str(toml_list).unwrap();
        assert_eq!(cfg_list.packs.default_behavior, "list");

        let toml_def = "[packs]\ndefault_behavior = \"default\"";
        let cfg_def: GlobalConfig = toml::from_str(toml_def).unwrap();
        assert_eq!(cfg_def.packs.default_behavior, "default");

        let toml_err = "[packs]\ndefault_behavior = \"error\"";
        let cfg_err: GlobalConfig = toml::from_str(toml_err).unwrap();
        assert_eq!(cfg_err.packs.default_behavior, "error");
    }

    #[test]
    fn test_parse_packs_multi_pack_table() {
        let toml_str = r#"
[packs.default]
description = "Default UI bundle"
components = ["toggle-theme", "btn-ally"]

[packs.wc-ui]
description = "Full UI bundle"
components = ["toggle-theme", "btn-ally", "wc-modal"]
"#;
        let packs = parse_packs_from_toml(toml_str, "packs");
        assert_eq!(packs.len(), 2, "Must parse 2 packs from [packs.<name>] table");
        assert_eq!(packs[0].name, "default");
        assert_eq!(packs[0].components, vec!["toggle-theme", "btn-ally"]);
        assert_eq!(packs[1].name, "wc-ui");
        assert_eq!(packs[1].components, vec!["toggle-theme", "btn-ally", "wc-modal"]);
    }

    #[test]
    fn test_parse_packs_array_of_tables() {
        let toml_str = r#"
[[packs]]
name = "default"
components = ["toggle-theme"]

[[packs]]
name = "full"
components = ["toggle-theme", "wc-modal"]
"#;
        let packs = parse_packs_from_toml(toml_str, "packs");
        assert_eq!(packs.len(), 2, "Must parse 2 packs from [[packs]] array");
        assert_eq!(packs[0].name, "default");
        assert_eq!(packs[1].name, "full");
    }

    #[test]
    fn test_parse_packs_top_level_tables() {
        let toml_str = r#"
[default]
components = ["toggle-theme"]

[wc-ui]
components = ["toggle-theme", "btn-ally"]
"#;
        let packs = parse_packs_from_toml(toml_str, "packs");
        assert_eq!(packs.len(), 2, "Must parse 2 packs from top-level tables");
        assert_eq!(packs[0].name, "default");
        assert_eq!(packs[1].name, "wc-ui");
    }

    #[test]
    fn test_recipe_inline_packs() {
        let toml_str = r#"
[recipes.wc-lib]
name = "Web Components"
description = "Modular components"

[recipes.wc-lib.packs.default]
components = ["toggle-theme"]

[recipes.wc-lib.packs.wc-ui]
components = ["toggle-theme", "btn-ally"]
"#;
        let cfg: Config = toml::from_str(toml_str).unwrap();
        let recipe = &cfg.recipes["wc-lib"];
        assert_eq!(recipe.packs.len(), 2);
        assert!(recipe.is_pack_recipe(), "Recipe with inline packs must be recognized as pack recipe");

        let pack = Config::find_pack(Some(recipe), "nonexistent_dir", "wc-ui").unwrap();
        assert_eq!(pack.name, "wc-ui");
        assert_eq!(pack.components, vec!["toggle-theme", "btn-ally"]);
    }

    #[test]
    fn test_provision_example_creates_config_toml_with_english_comments() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let count = COUNTER.fetch_add(1, Ordering::Relaxed);
        let user_dir = std::env::temp_dir().join(format!("fa-test-prov-{}-{}", std::process::id(), count));
        let _ = fs::remove_dir_all(&user_dir);

        Config::provision_example(&user_dir).unwrap();

        let config_toml_path = user_dir.join("config.toml");
        assert!(config_toml_path.exists(), "provision_example must create config.toml");

        let content = fs::read_to_string(&config_toml_path).unwrap();
        assert!(content.contains("[packs]"));
        assert!(content.contains("default_behavior = \"list\""));
        assert!(content.contains("# Behavior when running"));
        assert!(!content.contains("Comportamiento"));
        assert!(content.contains("[alias]"));
        assert!(content.contains("rn = \"--recipe new\""));
    }

    #[test]
    fn test_parse_global_config_git_style_and_toml_style() {
        let git_style = r#"
[packs]
default_behavior = "list"

[alias]
    b = branch
    s = switch
    st = status
    ac = !git add -A && git commit -m
"#;
        let global = parse_global_config(git_style).expect("Git-style unquoted aliases must parse");
        assert_eq!(global.alias.get("b"), Some(&"branch".to_string()));
        assert_eq!(global.alias.get("s"), Some(&"switch".to_string()));
        assert_eq!(global.alias.get("st"), Some(&"status".to_string()));
        assert_eq!(global.alias.get("ac"), Some(&"!git add -A && git commit -m".to_string()));

        let toml_style = r#"
[alias]
rn = "--recipe new"
ac = "!git add -A && git commit -m"
"#;
        let global_toml = parse_global_config(toml_style).expect("Standard quoted TOML aliases must parse");
        assert_eq!(global_toml.alias.get("rn"), Some(&"--recipe new".to_string()));
        assert_eq!(global_toml.alias.get("ac"), Some(&"!git add -A && git commit -m".to_string()));

        // Test sync_config_aliases populates config.aliases["config"] for '!' shell aliases
        let mut config = Config::default();
        config.settings = global;
        config.sync_config_aliases();
        assert!(config.aliases.contains_key("config"));
        let ac_cmd = &config.aliases["config"]["ac"];
        assert_eq!(ac_cmd.command, "git add -A && git commit -m");
        assert_eq!(
            config.resolve_command("ac").map(|(_, k, c)| (k, c.command.as_str())),
            Some(("ac".to_string(), "git add -A && git commit -m"))
        );
    }

    #[test]
    fn test_resolve_command_namespaced_subcommands_and_isolation() {
        let toml_content = r#"
[aliases.":skills"]
skills = { command = "tabernaculo status", description = "Root skills command" }
ls = { command = "bunx tabernaculo list", description = "List skills" }
add = { command = "bunx tabernaculo add", description = "Add skill", aliases = ["a"] }

[aliases.":docker"]
up = { command = "docker compose up -d" }
down = { command = "docker compose down" }

[aliases.general]
free = { command = "free -h" }
"#;
        let config: Config = toml::from_str(toml_content).expect("Should parse namespaced aliases");

        // 1. Explicit subcommands via space
        let res = config.resolve_command("skills ls");
        assert!(res.is_some(), "skills ls must resolve");
        let (sec, key, cmd) = res.unwrap();
        assert_eq!(sec, ":skills");
        assert_eq!(key, "ls");
        assert_eq!(cmd.command, "bunx tabernaculo list");

        // 2. Colon prefix in query
        let res = config.resolve_command(":skills ls");
        assert!(res.is_some(), ":skills ls must resolve");
        let (sec, key, _) = res.unwrap();
        assert_eq!(sec, ":skills");
        assert_eq!(key, "ls");

        // 3. Subcommand alias resolution
        let res = config.resolve_command("skills a");
        assert!(res.is_some(), "skills a must resolve to add");
        let (_, key, _) = res.unwrap();
        assert_eq!(key, "add");

        // 4. Root command invocation (same name as namespace)
        let res = config.resolve_command("skills");
        assert!(res.is_some(), "skills must resolve to root command in :skills");
        let (sec, key, cmd) = res.unwrap();
        assert_eq!(sec, ":skills");
        assert_eq!(key, "skills");
        assert_eq!(cmd.command, "tabernaculo status");

        // 5. Namespace without matching root command must return None on root query
        assert!(
            config.resolve_command("docker").is_none(),
            "docker has no root command, must return None"
        );
        assert!(config.resolve_command(":docker").is_none());

        // 6. Strict Isolation: Subcommands in :namespace must NOT leak into flat root namespace
        assert!(
            config.resolve_command("ls").is_none(),
            "ls is namespaced in :skills and must NOT leak to flat query"
        );
        assert!(
            config.resolve_command("add").is_none(),
            "add is namespaced in :skills and must NOT leak to flat query"
        );
        assert!(
            config.resolve_command("up").is_none(),
            "up is namespaced in :docker and must NOT leak to flat query"
        );

        // 7. General non-namespaced aliases still work normally
        let res = config.resolve_command("free");
        assert!(res.is_some(), "free in general section must resolve");
        let (sec, key, _) = res.unwrap();
        assert_eq!(sec, "general");
        assert_eq!(key, "free");
    }

    #[test]
    fn test_annotate_sources_tracks_file_and_line() {
        let content = r#"
[recipes.my-app]
name = "My App"
description = "Desc"

[aliases.":skills"]
skills = { command = "tabernaculo status", description = "Root" }
ls = { command = "bunx tabernaculo list", description = "List" }

[aliases.general]
free = { command = "free -h", description = "Free" }
"#;
        let mut config: Config = toml::from_str(content).unwrap();
        let path = PathBuf::from("/home/user/.config/fa/recipes.d/skills.toml");
        annotate_sources(&mut config, &path, content);

        let recipe = config.recipes.get("my-app").unwrap();
        assert_eq!(recipe.source_file.as_ref(), Some(&path));
        assert_eq!(recipe.source_line, Some(2));

        let ls_cmd = &config.aliases[":skills"]["ls"];
        assert_eq!(ls_cmd.source_file.as_ref(), Some(&path));
        assert_eq!(ls_cmd.source_line, Some(8));
    }

    #[test]
    fn test_expand_env_value() {
        let orig_home = std::env::var("HOME").ok();
        unsafe { std::env::set_var("HOME", "/custom/home"); }
        unsafe { std::env::set_var("MY_TEST_VAR", "hello"); }

        let expanded = expand_env_value("$HOME/bin:$MY_TEST_VAR");
        assert_eq!(expanded, "/custom/home/bin:hello");

        let tilde_expanded = expand_env_value("~/models/coder.gguf");
        assert_eq!(tilde_expanded, "/custom/home/models/coder.gguf");

        if let Some(h) = orig_home {
            unsafe { std::env::set_var("HOME", h); }
        }
        unsafe { std::env::remove_var("MY_TEST_VAR"); }
    }

    #[test]
    fn test_resolve_alias_env_fallback_and_force() {
        let toml_str = r#"
[aliases.":ai"]
_env = { CONTEXT = "2048", FALLBACK_VAR = "ns_fallback" }
_env_force = { BACKEND = "llama-cpp", FORCE_VAR = "ns_force" }

coder = { command = "llama-cli", env = { FALLBACK_VAR = "cmd_fallback", CMD_ONLY = "1" }, env_force = { FORCE_VAR = "cmd_force", MODEL_PATH = "/models/coder.gguf" } }
"#;
        let config: Config = toml::from_str(toml_str).expect("Should deserialize config with _env and _env_force");

        // Verify deserialization
        assert_eq!(config.alias_env[":ai"]["CONTEXT"], "2048");
        assert_eq!(config.alias_env[":ai"]["FALLBACK_VAR"], "ns_fallback");
        assert_eq!(config.alias_env_force[":ai"]["BACKEND"], "llama-cpp");
        assert_eq!(config.alias_env_force[":ai"]["FORCE_VAR"], "ns_force");

        let cmd = &config.aliases[":ai"]["coder"];
        assert_eq!(cmd.env["FALLBACK_VAR"], "cmd_fallback");
        assert_eq!(cmd.env_force["MODEL_PATH"], "/models/coder.gguf");

        // Test resolution:
        unsafe { std::env::set_var("CONTEXT", "8192"); }
        unsafe { std::env::set_var("BACKEND", "vllm"); }

        let env_list = resolve_alias_env(&config, ":ai", cmd);
        let env_map: std::collections::HashMap<String, String> = env_list.into_iter().collect();

        // 1. CONTEXT is in _env (fallback), but system already has CONTEXT=8192 -> must NOT be injected
        assert!(!env_map.contains_key("CONTEXT"), "Fallback variable already set in system must not be injected");

        // 2. CMD_ONLY is in cmd.env (fallback) and not in system -> must be injected
        assert_eq!(env_map.get("CMD_ONLY").map(|s| s.as_str()), Some("1"));

        // 3. BACKEND is in _env_force and system has BACKEND=vllm -> must be injected with llama-cpp
        assert_eq!(env_map.get("BACKEND").map(|s| s.as_str()), Some("llama-cpp"));

        // 4. FORCE_VAR is in cmd.env_force ("cmd_force") and _env_force ("ns_force") -> cmd.env_force wins
        assert_eq!(env_map.get("FORCE_VAR").map(|s| s.as_str()), Some("cmd_force"));

        // 5. MODEL_PATH is in cmd.env_force -> must be injected
        assert_eq!(env_map.get("MODEL_PATH").map(|s| s.as_str()), Some("/models/coder.gguf"));

        unsafe { std::env::remove_var("CONTEXT"); }
        unsafe { std::env::remove_var("BACKEND"); }
    }

    #[test]
    fn test_parse_env_string_bash_export_format() {
        let single = parse_env_string("export AI_IMAGE_MODEL=\"$AI_MODELS_DIR/vision/model.safetensors\"");
        assert_eq!(
            single.get("AI_IMAGE_MODEL").map(|s| s.as_str()),
            Some("$AI_MODELS_DIR/vision/model.safetensors")
        );

        let multi = parse_env_string("export MODEL=\"qwen.gguf\" && export THREADS=\"8\"");
        assert_eq!(multi.get("MODEL").map(|s| s.as_str()), Some("qwen.gguf"));
        assert_eq!(multi.get("THREADS").map(|s| s.as_str()), Some("8"));

        let no_export = parse_env_string("FOO=bar && BAZ='hello world'");
        assert_eq!(no_export.get("FOO").map(|s| s.as_str()), Some("bar"));
        assert_eq!(no_export.get("BAZ").map(|s| s.as_str()), Some("hello world"));
    }

    #[test]
    fn test_vars_and_bash_export_integration() {
        let toml_str = r#"
[vars]
GLOBAL_BIN = "sd-cli"

[aliases.":ai"]
_vars = { SD = "{{GLOBAL_BIN}} --steps 25 -m $AI_IMAGE_MODEL", SD_DESCRIPTION = "Generar imagen con SD" }

create-img-anima = { description = "{{SD_DESCRIPTION}} estilo anime: <prompt> <output>", command = "{{SD}}", env_force = 'export AI_IMAGE_MODEL="$AI_MODELS_DIR/vision/anima.safetensors"' }
"#;
        let config: Config = toml::from_str(toml_str).expect("Should parse TOML with _vars and bash-style env_force");

        let cmd = &config.aliases[":ai"]["create-img-anima"];
        assert_eq!(
            cmd.command,
            "sd-cli --steps 25 -m $AI_IMAGE_MODEL",
            "{{SD}} must be replaced by expanded template variable"
        );
        assert_eq!(
            cmd.description.as_deref(),
            Some("Generar imagen con SD estilo anime: <prompt> <output>"),
            "{{SD_DESCRIPTION}} must be replaced in description"
        );
        assert_eq!(
            cmd.env_force.get("AI_IMAGE_MODEL").map(|s| s.as_str()),
            Some("$AI_MODELS_DIR/vision/anima.safetensors"),
            "env_force bash string must be parsed into env_force map"
        );
    }

    #[test]
    fn test_load_lenient_captures_errors_and_loads_valid_recipes() {
        let temp = std::env::temp_dir().join(format!("fa-test-lenient-{}", std::process::id()));
        let recipes_d = temp.join("recipes.d");
        let _ = fs::create_dir_all(&recipes_d);

        let valid_path = recipes_d.join("valid.toml");
        fs::write(
            &valid_path,
            "[recipes.good]\nname = \"Good Recipe\"\ndescription = \"Testing good\"\n",
        )
        .unwrap();

        let broken_path = recipes_d.join("broken.toml");
        fs::write(
            &broken_path,
            "[aliases.ai]\n_SD = \"plain string not struct\"\n",
        )
        .unwrap();

        let mut config = Config::default();
        let mut loaded = Vec::new();
        let mut errors = Vec::new();

        Config::load_directory_lenient(&mut config, &recipes_d, &mut loaded, &mut errors);

        assert_eq!(errors.len(), 1, "Expected exactly 1 error for broken.toml");
        assert!(errors[0].contains("broken.toml"));
        assert!(config.recipes.contains_key("good"), "Valid recipe 'good' should be loaded despite broken.toml");
        assert_eq!(loaded.len(), 1);

        let _ = fs::remove_dir_all(&temp);
    }

    #[test]
    fn test_alias_singular_table_standalone() {
        let toml_str = r#"
[alias.wrapper.upscayl]
command = "flatpak run org.upscayl.Upscayl"
description = "AI image upscaler"
"#;
        let config: Config = toml::from_str(toml_str).expect("Should parse TOML with [alias.section.cmd]");
        let cmd = config
            .aliases
            .get("wrapper")
            .and_then(|w| w.get("upscayl"))
            .expect("wrapper.upscayl must exist in config.aliases");
        assert_eq!(cmd.command, "flatpak run org.upscayl.Upscayl");
        assert_eq!(cmd.description.as_deref(), Some("AI image upscaler"));
    }

    #[test]
    fn test_alias_and_aliases_coexist_and_merge() {
        let toml_str = r#"
[alias.sec]
foo = { command = "echo foo" }

[aliases.sec]
bar = { command = "echo bar" }

[alias.other]
baz = { command = "echo baz" }
"#;
        let config: Config = toml::from_str(toml_str).expect("Should parse TOML with both [alias] and [aliases]");
        let sec = config.aliases.get("sec").expect("section 'sec' must exist");
        assert!(sec.contains_key("foo"), "foo from [alias.sec] must be present");
        assert!(sec.contains_key("bar"), "bar from [aliases.sec] must be present");
        assert!(config.aliases.contains_key("other"), "other from [alias.other] must be present");
    }

    #[test]
    fn test_alias_source_annotation() {
        let toml_str = r#"
[alias.wrapper.upscayl]
command = "flatpak run org.upscayl.Upscayl"

[alias.tools]
tool1 = { command = "echo 1" }
"#;
        let mut config: Config = toml::from_str(toml_str).expect("Should parse config");
        let dummy_path = PathBuf::from("/dummy/recipes.d/tools.toml");
        annotate_sources(&mut config, &dummy_path, toml_str);

        let upscayl = &config.aliases["wrapper"]["upscayl"];
        assert_eq!(upscayl.source_file, Some(dummy_path.clone()));
        assert_eq!(upscayl.source_line, Some(2));

        let tool1 = &config.aliases["tools"]["tool1"];
        assert_eq!(tool1.source_file, Some(dummy_path));
        assert_eq!(tool1.source_line, Some(6));
    }

    #[test]
    fn test_command_args_list_of_tokens() {
        let toml_str = r#"
[aliases.img.convert]
command = "convert $1 $2"
args = ["<input>", "[output]"]
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        let cmd = &config.aliases["img"]["convert"];
        assert_eq!(cmd.args.len(), 2);
        assert_eq!(cmd.args[0].name, "input");
        assert!(cmd.args[0].required);
        assert_eq!(cmd.args[0].description, None);
        assert_eq!(cmd.args[1].name, "output");
        assert!(!cmd.args[1].required);
        assert_eq!(cmd.argument_signature(), Some("<input> [output]".to_string()));
    }

    #[test]
    fn test_command_args_list_of_objects() {
        let toml_str = r#"
[aliases.img.convert]
command = "convert $1 $2"
args = [
    { name = "input", description = "Input image path" },
    { name = "output", description = "Output directory", required = false }
]
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        let cmd = &config.aliases["img"]["convert"];
        assert_eq!(cmd.args.len(), 2);
        assert_eq!(cmd.args[0].name, "input");
        assert!(cmd.args[0].required);
        assert_eq!(cmd.args[0].description.as_deref(), Some("Input image path"));
        assert_eq!(cmd.args[1].name, "output");
        assert!(!cmd.args[1].required);
        assert_eq!(cmd.args[1].description.as_deref(), Some("Output directory"));
        assert_eq!(cmd.argument_signature(), Some("<input> [output]".to_string()));
    }

    #[test]
    fn test_command_args_table_numeric_keys() {
        let toml_str = r#"
[aliases.img.convert]
command = "convert $1 $2"
description = "Convert image"

[aliases.img.convert.args]
1 = { name = "input", description = "Input image path" }
2 = { name = "output", description = "Output directory", required = false }
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        let cmd = &config.aliases["img"]["convert"];
        assert_eq!(cmd.args.len(), 2);
        assert_eq!(cmd.args[0].name, "input");
        assert!(cmd.args[0].required);
        assert_eq!(cmd.args[1].name, "output");
        assert!(!cmd.args[1].required);
        assert_eq!(cmd.argument_signature(), Some("<input> [output]".to_string()));
    }

    #[test]
    fn test_command_args_table_named_keys() {
        let toml_str = r#"
[aliases.img.convert]
command = "convert $1 $2"

[aliases.img.convert.args]
input = { description = "Input image path" }
output = { description = "Output directory", required = false }
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        let cmd = &config.aliases["img"]["convert"];
        assert_eq!(cmd.args.len(), 2);
        assert_eq!(cmd.args[0].name, "input");
        assert!(cmd.args[0].required);
        assert_eq!(cmd.args[1].name, "output");
        assert!(!cmd.args[1].required);
        assert_eq!(cmd.argument_signature(), Some("<input> [output]".to_string()));
    }

    #[test]
    fn test_command_args_single_string_fails() {
        let toml_str = r#"
[aliases.img.convert]
command = "convert $1 $2"
args = "<input> [output]"
"#;
        let res: Result<Config, _> = toml::from_str(toml_str);
        assert!(res.is_err(), "Single string for args must be rejected");
    }

    #[test]
    fn test_command_args_template_var_substitution() {
        let toml_str = r#"
[vars]
EXT = "avif"

[aliases.img.convert]
command = "convert $1 $2"
args = [
    { name = "input", description = "Input {{EXT}} file" }
]
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        let cmd = &config.aliases["img"]["convert"];
        assert_eq!(cmd.args[0].description.as_deref(), Some("Input avif file"));
    }

    #[test]
    fn test_toml_1_1_multiline_inline_tables_and_trailing_commas() {
        let toml_str = r#"
[aliases.demo]
hello = {
    command = "echo 'Hello TOML 1.1!'",
    description = "Multiline inline table with trailing comma",
}
"#;
        let config: Config = toml::from_str(toml_str).expect("TOML 1.1 multiline inline table should parse cleanly");
        let cmd = &config.aliases["demo"]["hello"];
        assert_eq!(cmd.command, "echo 'Hello TOML 1.1!'");
        assert_eq!(cmd.description.as_deref(), Some("Multiline inline table with trailing comma"));
    }
}


