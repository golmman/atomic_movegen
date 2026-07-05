# How to Write Proper Rustdocs

**Source:** https://doc.rust-lang.org/rustdoc/

> All public API items **must** be documented. If an item is public, it should have docs.

## Crate-level docs (`//!`)

Place at the top of `lib.rs` with `//!` (inner doc comments). Structure:

1. One-line summary — what the crate does and where it fits in the ecosystem.
2. Detailed explanation of the crate's role.
3. A copy-pasteable code example.
4. Links to technical details, feature flags, etc.

## Item-level docs (`///`)

Use `///` (outer doc comments). Recommended structure per item:

```
/// [short sentence explaining what it is]
///
/// [more detailed explanation]
///
/// # Examples
///
/// ```rust
/// // code example that users can copy/paste
/// ```
///
/// # Panics
///
/// [edge cases that cause panics, if any]
```

- The first paragraph (before the first blank line) is the **summary** — it appears in module overviews and search results. Keep it one line.
- Use `# Examples`, `# Panics`, `# Errors`, `# Safety` sections with third-level headings.
- Do **not** re-state the type signature — rustdoc auto-links types in signatures.

## Intra-doc links

Link to items by name using ``[`ItemName`]`` or the standard Markdown link syntax:

```rust
/// See also: [`std::fs::read_to_string`]
/// See also: [crate::module::Type]
/// See also: [positional parameters](std::fmt#formatting-parameters)
```

- Backticks around links are stripped automatically.
- Supports `Self`, `self`, `super`, `crate` paths.
- **Disambiguate** when names overlap: `struct@Foo`, `fn@Foo`, `enum@Foo`, `trait@Foo`, `macro!()`, `fn()`, etc.
- Links resolve in the scope where the item is **defined**, not where it is re-exported.
- Use `#![deny(rustdoc::broken_intra_doc_links)]` to catch broken links at build time.

## Relevant lints

Add to `lib.rs` to enforce documentation quality:

| Lint | Default | Purpose |
|------|---------|---------|
| `#![deny(missing_docs)]` | allow | Force all public items to have docs. |
| `#![deny(rustdoc::broken_intra_doc_links)]` | warn | Catch broken intra-doc links. |
| `#![deny(rustdoc::private_intra_doc_links)]` | warn | Catch public-to-private links. |
| `#![warn(rustdoc::bare_urls)]` | warn | URLs that are not hyperlinked. |
| `#![warn(rustdoc::invalid_html_tags)]` | warn | Unclosed/invalid HTML tags. |
| `#![warn(rustdoc::invalid_rust_codeblocks)]` | warn | Unparseable Rust code blocks. |
| `#![warn(rustdoc::redundant_explicit_links)]` | warn | Explicit links identical to auto-computed ones. |
| `#![warn(rustdoc::unescaped_backticks)]` | allow | Broken inline-code backticks. |

## Documentation tests (doctests)

- Code blocks in docs are compiled and run via `cargo test`.
- Use `` ``` `` (no language tag) or `` ```rust `` — both are treated as Rust.
- Lines starting with `# ` are **hidden** from output but still compiled (for setup code).
- Use `# fn main() {}` or `# Ok::<(), Error>(())` to make examples with `?` compile.
- **Code block attributes:**
  - ` ```ignore ` — skip compiliation (last resort; prefer `#`-hiding).
  - ` ```should_panic ` — test must panic.
  - ` ```no_run ` — compile but don't run (e.g. network examples).
  - ` ```compile_fail ` — compilation must fail.
  - ` ```edition2024 ` — use a specific edition.

## Markdown features

rustdoc uses **CommonMark** plus these extensions:

- **Strikethrough:** `~~text~~` or `~text~`
- **Footnotes:** `[^note]` with definition `[^note]: Text`
- **Tables:** GFM pipe-table syntax.
- **Task lists:** `- [x]` / `- [ ]`
- **Smart punctuation:** `--` → en-dash, `---` → em-dash, `...` → ellipsis.
- **Warning blocks:** `<div class="warning">Beware!</div>`

## The `#[doc]` attribute

- `#[doc(hidden)]` — hide an item from public docs (useful for internal macros, implementation details).
- `#[doc = include_str!("../README.md")]` — inline external Markdown files (useful for testing README doctests).
- `#[doc(test(attr(...)))]` — add attributes to all doctests (e.g. `warn(unused)`).

## Re-exports

Re-exported items retain their original docs. Additional docs can be added on the re-export, and their intra-doc links resolve in the re-export's scope.

## What to avoid

- **Bare URLs** — wrap them in `<>` to make them clickable.
- **Unwrapped `unwrap()` in examples** — prefer `?` with hidden `# fn main() -> Result<...>`.
- **Empty code blocks** — always provide runnable examples.
- **Explicit links that match the auto-link** — they are redundant and trigger a lint warning.
- **Missing `main()` or `extern crate` in doctests** — rustdoc adds them, but macros and `?` need manual handling.
