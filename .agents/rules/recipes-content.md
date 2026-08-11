# Rule: Recipe Inline Content Size Limit

Recipe file entries declared with `inline` in `recipes.toml` (or `recipes.d/*.toml`) MUST NOT exceed **40 lines** of content.

## Principles

1. **Hard Cap at 40 Lines**:
   - Any file declared as `{ inline = """...""" }` whose content is longer than 40 lines is rejected by convention. Rewrite it.
   - The line count is measured over the file content itself, not the TOML wrapper.

2. **Prefer Template Files**:
   - Long files MUST be moved to a standalone template file under `~/.config/fa/templates/<recipe-name>/` and referenced with `{ from = "templates/<recipe-name>/<file>" }`.
   - Template files are read from disk at runtime — no Rust code changes required.

3. **Short Files Stay Inline**:
   - Small, one-off files (`.gitkeep`, minimal configs, short components) may remain inline. Keep the recipe file readable.

4. **Special Cases Require User Approval**:
   - If a file genuinely must stay inline and exceed 40 lines, STOP and ask the user for approval before proceeding. Do not silently exceed the cap.

5. **Why**:
   - Inline content bloats `recipes.toml`, making recipes hard to read, diff, and maintain. Template files keep the catalog declarative and readable.
