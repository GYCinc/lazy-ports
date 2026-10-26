---
plan: skin-self-contained-industrial-default
status: PROPOSED
base_commit: 15153f3
created: 2026-10-26T10:53:49-06:00
consumer: opencode-ai
---

# A. Discovery Log

## K (Knowledge — from the prompt alone)

- App is a COSMIC panel applet ("Lazy Ports", Rust, libcosmic, `src/app.rs` 3073 lines).
- User asked earlier for the app to keep the option of a custom "skin" (Industrial look) and for it to **MUST be skinned**, while the applet template dictates it follows the system theme default "of whichever user" — templated applets "are all like this and no agent can comprehend" the app's intent.
- User explicitly: keep the skin **option** if allowed, **remove no functions at all**, and **upload ASAP** — the skin work is what's blocking the upload.
- User additionally reported the Industrial layout is "not worth keeping" as-is, and that on-screen results "look like shit".
- Screenshots show two concrete defects: (a) Preferences — `settings::item` rows collapse the label column so title/description wrap into a narrow ribbon while the control sits far right; (b) Ports list — rows are visually unstructured, port/PID typography is weak, action icon glyphs are near-invisible.
- User: long service names are "not wrapped or cut off soon enough and smashed in to the buttons".
- User: "stop defaulting to make all the listening ports OPEN ... its a collapsing menu and it defaults to open".
- User: "so many templates and examples" + pointed at `/home/hayron/MyProjects/cosmic-applet-template` and `https://github.com/pop-os/cosmic-applets` — wants the app to conform to established COSMIC applet idioms rather than a bespoke one.

## W (Wonders)

### W-Blocking
1. **Is a bespoke skin allowed / wanted, given the "follow the system theme" template guidance?**
   → **Resolved from L, not blocking.** `libcosmic` exposes no restriction; `cosmic::applet::style()` is a *convenience* returning the system palette, and `Application::style()` is an `Option<theme::Style>` hook (libcosmic `src/app/mod.rs:496`, consumed at `src/app/cosmic.rs:557-570`) precisely so an app can override it. Both `pop-os/cosmic-applets` and the local `cosmic-applet-template` use `cosmic::applet::style()`. **Answer to user: yes, allowed** — and the template is guidance, not enforcement.

### W-Non-Blocking
2. Keep the Industrial *option* switchable, or make the skin mandatory? → **Assumption A1:** Industrial becomes the **default and the app's identity**; the System option is **retained** as a user-selectable fallback (satisfies "keep the option" + "remove no functions at all").
3. Is "upload" a GitHub push or a COSMIC store submission? → **Assumption A2:** upload target = **Push to `github.com/hayron/lazy-ports`** (matching the metainfo URL + app ID `io.github.hayron.lazy-ports`). Not a `pop-os/cosmic-applets` PR: that repo is upstream GPL-3.0 and MIT applets are not merged there.
4. Should the *system* theme path be deleted? → **Assumption A3:** **No deletions** (explicit user instruction). Both paths retained.
5. Which visual system replaces the current Industrial layout? → **Assumption A4:** Rebuild on the **COSMIC native widget vocabulary** (`settings::section/item`, `Container::Card`, `Button::*`) — since the user said "so many templates and examples" and the official applets do exactly this — while keeping a **dark instrument chassis** as the app's signature so it still reads as *skinned*, and it must be legible in **light** system themes too.
6. Does the Industrial *look* survive at all? → **Assumption A5:** Yes, as a **skin layer**: the dark chassis, hairline edges, mono telemetry, LED status dots, and accent colour persist as the app's identity; the hand-rolled card/row geometry is what gets replaced by COSMIC-idiomatic layout primitives.

## L (Learnings — verified by inspection)

### Skin / theming surface
- `src/app.rs:2166` — `fn style(&self) -> Option<cosmic::iced::theme::Style> { Some(cosmic::applet::style()) }`. **This is the single line that makes the app follow the system theme** for text/icon colour.
- `src/app.rs:3052` — `fn industrial(&self) -> bool { self.config.use_industrial() }`.
- `src/config.rs:55` — `use_industrial()` returns `theme_mode.trim().eq_ignore_ascii_case("industrial")`; empty ⇒ **system**. (Commit `15153f3` flipped this from industrial-by-default.) `[L: config.rs:55]`
- `src/config.rs:26` — `theme_mode: String` — doc comment says *"Empty means system"*.
- `src/app.rs:2055-2063` — `Message::SetThemeMode` normalises to `"industrial"` / `"system"` and persists via `ctx.set("theme_mode", ...)`.
- `src/app.rs:1247-1271` — Preferences "Theme" row: `System` / `Industrial` buttons; `industrial` ⇒ Industrial is `Button::Suggested`.
- Live config `~/.config/cosmic/io.github.hayron.lazy-ports/v1/theme_mode` = `"system"` — **the running app is in System mode**, i.e. Commit `15153f3` is why the user's screenshots show the generic look.
- libcosmic `src/app/cosmic.rs:557-570` — `if let Some(style) = self.app.style() { style } else { … system colours … }` → returning `Some(custom)` fully replaces system text/icon colour.
- libcosmic `src/applet/mod.rs:596-603` — `applet::style()` reads `crate::theme::THEME` (system-derived) and returns `background_color: transparent`, `text_color`/`icon_color` = system `on_bg_color`.
- libcosmic `src/theme/mod.rs:57-86` — `active()`, `active_type()`, `is_dark()`; `:205-255` — `Theme::dark()`, `Theme::light()`, `Theme::custom(Arc<CosmicTheme>)`, `Theme::system(Arc<CosmicTheme>)`.
- libcosmic `src/command.rs:36-38` — `pub fn set_theme<M>(theme: crate::Theme) -> iced::Task<crate::Action<M>>` (gated on `winit` feature, **enabled** in this repo's `Cargo.toml`) → dispatches `Action::AppThemeChange`, handled at `src/app/cosmic.rs:911`. This is the sanctioned way to install a **custom dark theme** at runtime even under a light system theme.
- `cosmic::applet::style()` returns `background_color: Color{0,0,0,0}` (transparent) — the popup window is transparent, so the **chassis plate** (`src/app.rs:870-882`, `#14161b`) is what actually paints the app's background. System-theme backgrounds never bleed through it today.

### Layout defects visible in the screenshots
- `src/app.rs:1205-1290` — Preferences built from `settings::section()` + `settings::item::builder(...)`. libcosmic `src/widget/settings/item.rs:39-45` — `item_row` = `row::with_children([text(Fill), space, widget])`; `:117-126` — the label is a `column` with `Length::Fill`. Because the **control is `Shrink`-width and the label is `Fill`**, a wide control (three refresh-rate buttons, two theme buttons) squeezes the title/description column to a ~90 px ribbon → the observed vertical text-wrap ribbon in Image 1.
- libcosmic `src/widget/settings/item.rs:72-79` — `flex_item_row` uses `min_item_width(200.0)` + `justify_content: SpaceBetween` → `flex_control()` is the variant that reserves a readable label column instead of squeezing it.
- `src/app.rs:216-234` — `link_title()` builds a clickable title button; `src/app.rs:236-247` — `plain_title()`.
- `src/app.rs:814-871` — `card_shell()`: title row = `[container(title_left) width Fill clip(true)] + [actions_right]`. The title is `widget::Text` with `Wrapping::None` and **no explicit width**, so **long names overflow their `Fill` parent** instead of being clipped/ellipsised — the "smashed in to the buttons" defect. `container.clip(true)` only clips *painted* content, it does not constrain the child's intrinsic width, so the text pushes into the action cluster.
- libcosmic iced `iced/core/src/widget/text.rs:160-168` — `Text::wrapping(Wrapping)` and **`Text::ellipsize(Ellipsize)`**; `iced/core/src/text.rs:199-214` — `Ellipsize::{None, Start, Middle, End(EllipsizeHeightLimit)}`. **This is the correct fix for long names** (ellipsise instead of overflow).
- `src/app.rs:558-580` — `key_button_class()` / `key_style()`: `icon_color: Some(ink)`. At rest (`key_style_flat`) the glyph ink is `COLOR_TEXT2` `#aab4c0` at 14 px on a `#1b1f25` card ⇒ low-contrast, nearly invisible at rest (Image 2 complaint).
- `src/app.rs:565-573` — `key_button_class` mapping: Neutral→`COLOR_TEXT2`, Blue→`LED_CYAN`, Green→`accent.color`, Danger→`LED_RED`.
- `src/app.rs:570` — `flat_btn_style`/`key_style_flat` produce **no plate and no border at rest** ⇒ glyph floats on the card with no anchor.
- `src/app.rs:270-285` — `endpoint_line()` uses `Rich` with `Span`s; `src/app.rs:772-806` — `themed_endpoint()`.
- No `Ellipsize` usage anywhere in `src/app.rs` → `grep -c "ellipsize" src/app.rs` = **0**. No text in the app ever ellipsises.
- `src/app.rs:150-137` palette consts — `COLOR_CHASSIS #14161b`, `COLOR_SURFACE #1b1f25`, `COLOR_VIEWPORT #0c0e12`, `COLOR_BORDER #252932`, `COLOR_TEXT2 #aab4c0`, `COLOR_BRIGHT #e6edf3`, `COLOR_KEY #d4d4d8`, `COLOR_SUBTLE #626875`, `COLOR_DIM #8b93a0`.
- `src/app.rs:358-370` `badge_plate_widget()`; `:421-429` `drawer_plate()`; `:718-733` `row_card()`; `:735-743` `card_class()` (industrial ⇒ custom hairline plate, system ⇒ `Container::Card`); `:745-752` `drawer_class()`; `:754-768` `themed_badge()`; `:870-882` `chassis_plate()`; `:884-897` `panel_plate()`.

### The "listening ports default to open" defect
- `src/config.rs:20` — `pub show_ports: bool` ⇒ `#[derive(Default)]` ⇒ **`false`**.
- `src/app.rs:1658` — `if self.config.show_ports { … }` gates the whole Listening Ports rail body; `:1650-1656` renders the `section_rail(..., self.config.show_ports, Message::ToggleShowPorts)`.
- **Live config** `~/.config/cosmic/io.github.hayron.lazy-ports/v1/show_ports` = `true` (written 2026-10-19, *after* the 2026-10-02 batch) and `mtime` differs from every sibling key ⇒ the value was **persisted as `true` by a toggle write**, not by a default.
- ⇒ The **derive default is already correct**; the defect is that a *persisted* `true` survives and the user cannot get a clean default back. Confirmed by the user's own live file. The fix is a **config version bump + migration** that resets a previously-toggled `show_ports` to `false` on upgrade, plus persisting the default explicitly rather than relying on `derive(Default)`.

### Build / verify surface
- `justfile` — recipes: `build-debug` (`cargo build --locked`), `build-release`, `check` (`cargo clippy --all-features --locked -- -W clippy::pedantic`), `run`, `install`, `uninstall`, `vendor`, `tag`.
- **`just` is NOT installed on this machine** (`which just` → not found) ⇒ verification must use raw `cargo` commands, not `just`.
- `Cargo.toml` — libcosmic from git `pop-os/libcosmic`, features: `applet`, `applet-token`, `dbus-config`, `tokio`, `wayland`, `winit`. **`winit` is enabled ⇒ `cosmic::command::set_theme` is available.**
- `Cargo.lock` pins `combine 4.6.8` but only `combine 4.6.7` is in the local registry cache ⇒ **the first build must hit the network for one crate**; `cargo metadata --offline` fails today.
- Tests: `src/app.rs:3033` `mod tests` (2 tests), `src/config.rs` (2), `src/ports.rs` (1), `src/services.rs` (2) ⇒ **7 tests total**.
- `rustc 1.98.0`, `cargo 1.98.0`; edition 2024.

### Release / upload surface
- `resources/app.metainfo.xml` — id `io.github.hayron.lazy-ports`; `<url type="vcs-browser">https://github.com/hayron/lazy-ports</url>`; icon points at `https://github.com/hayron/lazy-ports/raw/main/resources/icons/hicolor/scalable/apps/io.github.hayron.lazy-ports-symbolic.svg`; description says *"It follows the system COSMIC theme by default, with an optional legacy industrial dark look available in Preferences."* → **must be rewritten** to describe the new skin-first behaviour, or the store listing contradicts the app.
- `resources/app.metainfo.xml` `<releases>` — single entry `version="0.1.0" date="2026-10-07"` describing the system-theme default ⇒ needs a new release entry.
- `Cargo.toml` version `0.1.0`; `justfile` `appid := 'io.github.hayron.lazy-ports'`; `src/app.rs:21` `APP_ID = "io.github.hayron.lazy-ports"`.
- **No git remote is configured** (`git config --get-regexp '^remote'` → empty). `gh auth status` → authenticated as **GYCinc**. `gh repo view hayron/lazy-ports` → **does not exist**. `gh repo view GYCinc/loPee` → **exists** (private), description "Lazy Ports", README = this project's README.
- ⇒ The upload target **`github.com/hayron/lazy-ports` (named in metainfo/Cargo.toml) does not exist**. The only existing remote-able repo is `GYCinc/loPee`. This is a genuine W-Non-Blocking resolution point: create `hayron/lazy-ports` under the authenticated account if the account owns it, else fall back to `GYCinc/lazy-ports` and update the metainfo/Cargo.toml URLs to match.

### Codebase idioms
- Palette is defined as `const` colour values at the top of `src/app.rs`, never as a struct/enum palette → new skin values follow the same const style.
- Theme-dependent widgets are chosen via `cosmic::theme::Container::Custom(Box::new(|_| Style { … }))` — the house pattern for bespoke surfaces.
- Every "theme-aware" helper takes a `industrial: bool` and branches internally (`card_class`, `drawer_class`, `themed_badge`, `themed_endpoint`, `plain_title`).
- User-visible strings live in `i18n/en/lazy-ports.ftl` **and** a duplicate `i18n/en/lazy_ports.ftl` (both present; `src/i18n.rs` picks one) — `fl!` macro used throughout.
- `logwork` CLI is the mandated logging mechanism (per workspace `AGENTS.md`); `WORKLOG.md` + `worklog.db` at repo root.

## H (How it Drives Success)

- **`L: config.rs:55` (empty ⇒ system) directly causes the user's screenshots.** Plan therefore replaces the boolean-flag duality with a **skin-first resolution**: `theme_mode` empty ⇒ *skin*; `"system"` ⇒ *system*; `"industrial"` ⇒ *skin* (legacy persisted value keeps working). This satisfies "keep the option" (System stays selectable), "MUST be skinned" (default), "remove no functions" (nothing deleted), and "no agent can comprehend" (one obvious default, documented).
- **`L: app.rs:2166` `Some(cosmic::applet::style())` is the mechanism by which system colour leaks into the app.** The plan overrides `style()` to return an explicit `theme::Style` (skin text/icon colours) whenever the skin is active, so the surface is theme-independent. The skin must be legible under a **light** system theme — proven by `L: theme/mod.rs:237` `Theme::custom(Arc<CosmicTheme>)` and `L: command.rs:36` `set_theme`, which install a real dark cosmic theme instead of only overriding the top-level style.
- **`L: settings/item.rs:39-45` (label `Fill` + control `Shrink`) exactly reproduces the squeezed Preferences ribbon in Image 1.** Fix by switching those rows to `flex_control()` (`:72-79`, `min_item_width(200.0)`) or by giving the label column an explicit `Length::Fixed`/guaranteed minimum. Verified remedy, not a guess.
- **`L: app.rs:814-871` + `iced/core/src/widget/text.rs:166` (`Text::ellipsize`) provide the proven fix for "smashed in to the buttons".** Long titles must be **ellipsised at the end** within a bounded width, with the action cluster given a reserved, non-shrinking width so the two can never overlap. `Container::clip(true)` is insufficient — it clips paint, not layout.
- **`L: app.rs:558-580`** explains "icon buttons are so light there aren't really visible": 14 px glyphs at `#aab4c0` with no rest plate. Fix = raise glyph ink to a brighter constant and give each action key a visible rest plate/border consistent with the milled-key concept already in `key_style()`.
- **`L: config.rs:20` + live `show_ports=true`** shows the "defaults to open" bug is a **stale persisted value**, not the derive default. Fix = bump `#[version]` and add a migration that forces `show_ports = false` once on upgrade, so the user's live install starts collapsed without deleting the toggle.
- **`L: metainfo.xml` description + release note** currently advertise the opposite of the new behaviour ⇒ the upload would ship a false listing. Plan rewrites both.
- **`L: no remote / hayron-lazy-ports does not exist`** is the concrete upload blocker beyond the skin → the plan includes the remote/repo creation step explicitly, gated on a repo-name decision resolved by the safest fallback (`GYCinc/lazy-ports` with matching metainfo URL updates).
- **`L: just` not installed + `combine 4.6.8` uncached** neutralise any `just`-based verification ⇒ plan uses raw `cargo` commands and its first gate expects exactly one network fetch.
- Remaining W-Non-Blocking items are converted to **Assumptions A1–A5**, carried into §4 B, and applied as: skin default + System retained (A1); push to the metainfo-named repo, falling back to `GYCinc/lazy-ports` (A2); no deletions (A3); COSMIC-idiomatic layout primitives as the base (A4); dark chassis/LED/mono accent retained as the signature skin (A5).
- **Confidence: 88%** — every defect in the user's screenshots maps to a verified code line, and every API used in the plan was read from the pinned libcosmic checkout. The residual 12% is the repo-name/upload-target uncertainty (A2) and the subjective quality bar of the rebuilt visual design.

---

# B. Context & Problem Statement

**Core problem.** Lazy Ports is a COSMIC panel applet whose UI must be a distinctive, self-owned **skinned** instrument panel, but commit `15153f3` made the **system** theme the default (`src/config.rs:55`) and `src/app.rs:2166` unconditionally returns `cosmic::applet::style()`, so the running binary hands text/icon colour to the system palette. The result is the generic look in the screenshots, with no way for a user (or agent) to tell what the app is supposed to look like.

**Current behaviour.** Theme mode resolves to *system* on a fresh/unset config → COSMIC-idiomatic containers → Preferences rows collapse into a narrow wrapped ribbon (`settings::item` label `Fill` vs. control `Shrink`), action glyphs render near-invisible (`#aab4c0`, no rest plate), long service names overflow into the button cluster (`Wrapping::None` + no ellipsis), and the Listening Ports rail opens by default because a persisted `show_ports=true` survives every upgrade.

**Target state.** The skin is the default *and* the app's identity: an explicit `style()` plus a runtime dark cosmic theme make the surface independent of the system theme in both light and dark systems; Preferences use layout primitives that guarantee a readable label column; every action key is legible at rest; long names ellipsise cleanly inside a reserved column; the Listening Ports rail starts collapsed; the metainfo describes the truth; and the tree is ready to push and release.

**Blast radius.**
- `src/config.rs` — skin resolution + config version bump.
- `src/app.rs` — `style()` override, theme-mode plumbing, Preferences layout, action-key styling, title ellipsis, rail default.
- `resources/app.metainfo.xml` — description + release entry (store listing truth).
- `README.md` — theme-behaviour line.
- **No functional removals.** No message type, service capability, port action, health check, log view, or preference is deleted. The `System` theme option remains selectable.

**Non-goals.**
- No new features, no new service/port capabilities, no new dependencies.
- No submission to `pop-os/cosmic-applets` (upstream GPL-3.0; MIT applets are not merged there).
- No redesign of the applet's panel-button icon or of the `res/fonts` CaveatBrush wordmark.
- No porting off `cosmic_config` persistence.

**Assumptions & Open Items.**
- **A1** — Industrial/skin becomes the **default**; the **System** option stays user-selectable in Preferences. "MUST be skinned" is read as *skinned by default, skin-first*, not *skin mandatory* — deleting the System path would violate "remove no functions at all".
- **A2** — Upload = push the tree to the repo named in the metainfo (`github.com/hayron/lazy-ports`); if that repo cannot be created under the authenticated `GYCinc` account, publish to `GYCinc/lazy-ports` and update the metainfo + `Cargo.toml` URLs to the actual location.
- **A3** — Zero feature removals, verified by the §F checklist.
- **A4** — The rebuilt UI uses COSMIC-native layout primitives (`settings::*`, `Container::Card`, `Button::*`); the bespoke boundary-clipped hand-rolled card geometry is replaced by them.
- **A5** — The industrial **skin layer** (dark chassis `#14161b`, hairline `#252932` borders, mono telemetry, 8 px status LEDs, user accent, milled 28 px keys) is retained as the app's signature; only the *organisation* is rebuilt.
- **Open O1** — Repo-name decision (A2) must be settled at Task 6; it changes only the remote URL and the metainfo URL, not the code.

---

# C. Architectural Approaches

## Option 1 (Primary) — Skin-first, COSMIC-idiomatic layout

Rebuild the surface on COSMIC's own layout primitives while installing an **explicit, self-owned theme**:

1. **Skin resolution is explicit, not a boolean default.** `Config` gains a `skin_name()`/`use_skin()` pair where *empty* ⇒ skin, `"system"` ⇒ system. The persisted legacy `"industrial"` value is accepted as skin so no existing user loses their look.
2. **`style()` returns an explicit style when the skin is active** (`src/app.rs:2166`), with skin-derived text/icon colours, so system palette never leaks into the skin.
3. **A dark cosmic theme is installed at runtime** via `cosmic::command::set_theme(Theme::custom(...))` when the skin is active, so `settings::item`, togglers, buttons and text inputs all resolve against the skin's own palette instead of the light system theme. This is what makes the skin look identical on a light system — the "no agent can comprehend" case.
4. **Preferences** use `flex_control()` (or an explicit minimum label width) so the label column can never collapse to a ribbon.
5. **Action keys** get a visible rest plate + brighter ink.
6. **Long names** ellipsise at the end inside a width-bounded column, with the action cluster reserved at a fixed width.
7. **Config version bump** migrates a stale `show_ports=true` to `false` once.

**Files:** `src/config.rs`, `src/app.rs`, `resources/app.metainfo.xml`, `README.md`. Four files, no new dependencies.

## Option 2 (Lean) — Surgical patch, no theme rewrite

Keep the current `industrial: bool` duality and layout, and patch only the four reported symptoms:
1. Flip the `use_industrial()` default back to skin-on-empty.
2. Give the Preferences label column an explicit min width.
3. Brighten the action glyph ink.
4. Ellipsise long titles.
5. Force `show_ports` false on startup.

**Files:** `src/config.rs`, `src/app.rs`. Two files.

## Hard tradeoffs

| | Option 1 (Primary) | Option 2 (Lean) |
|---|---|---|
| Implementation time | Medium — one theme-plumbing pass + four layout fixes | Low — five small edits |
| Cognitive overhead | Lower long-term: one documented "skin-first" rule replaces `if industrial` scattered across ~14 call sites | Higher: the `industrial` flag keeps branching, so every future widget needs two variants and the "no agent can comprehend" complaint persists |
| Failure modes | Runtime theme swap could fight the panel's own theme; mitigated by only installing a theme while the skin is active and by scoping it to the popup | Leaves light-system-theme legibility unresolved: `Container::Card`, `Button::Suggested`, togglers and text inputs still follow the system palette, so the skin reads as a *partially* skinned app — exactly the confusion reported |
| Feature risk | Low — no message/feature touched; `System` path retained | Low |
| "Upload ready" | Yes — the skin is coherent in both system themes and the listing can honestly describe it | Partially — screenshots still show a different app depending on the user's system theme |

**Decision rule.** Choose Option 2 only if the upload deadline is shorter than the time to build and visually verify a runtime theme swap; otherwise Option 1, because Option 2 leaves the app's appearance user-dependent, which is the complaint being fixed.

---

# D. Technical Blueprint (Primary Path)

## D1. Interfaces & Schemas

### `src/config.rs` — skin resolution

Existing type (unchanged shape, new semantics):

```rust
#[derive(Debug, Default, Clone, CosmicConfigEntry, Eq, PartialEq)]
#[version = 2]                      // was 1 — see D2
pub struct Config {
    pub services_json: String,
    pub refresh_secs: u64,
    pub ignored_json: String,
    pub colloquial: bool,
    pub hide_system: bool,
    pub show_ports: bool,
    pub accent: String,
    /// Theme mode: "" or "industrial" => skin (default); "system" => follow COSMIC.
    pub theme_mode: String,
}
```

New/changed methods on `impl Config`:

```rust
/// True when the industrial skin is active. Empty (unset) means the skin:
/// the applet ships skinned; "system" is the explicit opt-out.
pub fn use_industrial(&self) -> bool {
    !self.theme_mode.trim().eq_ignore_ascii_case("system")
}

/// Canonical persisted name for the active mode. The skin is written as
/// "industrial" so pre-v2 configs and the post-v2 default agree.
pub fn theme_mode_name(&self) -> &'static str {
    if self.use_industrial() { "industrial" } else { "system" }
}
```

> `use_industrial()` keeps its existing name and signature so every existing call site (`src/app.rs:3052`) compiles unchanged. Only the **default** flips: empty ⇒ skin.

Version bump helper (new, `src/config.rs`):

```rust
/// v1 -> v2 migration. The Listening Ports rail now starts collapsed, so a
/// v1 `show_ports = true` (which was the shipped default) is reset once.
/// Every other key carries over untouched.
pub fn migrate_from_v1(v1: &Config) -> Config {
    Config {
        show_ports: false,
        ..v1.clone()
    }
}
```

### `src/app.rs` — skin-aware helpers

New constants (added beside the existing palette block at `src/app.rs:26-137`):

```rust
/// Ink for a resting action glyph. Deliberately brighter than COLOR_TEXT2:
/// 14 px symbolic glyphs on the #1b1f25 card need #d4d4d8 to read as a
/// control rather than a smudge.
const COLOR_INK_ACTIVE: cosmic::iced::Color = cosmic::iced::Color {
    r: 0.831, g: 0.831, b: 0.847, a: 1.0,
}; // #d4d4d8 — same value as COLOR_KEY, named for its role
```

New/changed functions:

```rust
/// Title text that ellipsises instead of overflowing into the action keys.
/// `max_width` bounds the title column; the action cluster is reserved by
/// the caller, so the two can never overlap.
fn bounded_title<'a, M: 'static>(title: String, industrial: bool) -> Element<'a, M> {
    let txt = widget::Text::new(title)
        .size(13.0)
        .font(cosmic::font::semibold())
        .wrapping(cosmic::iced::core::text::Wrapping::None)
        .ellipsize(cosmic::iced::core::text::Ellipsize::End(
            cosmic::iced::core::text::EllipsizeHeightLimit::SingleLine,
        ));
    if industrial {
        txt.class(cosmic::theme::Text::Color(COLOR_BRIGHT)).into()
    } else {
        txt.into()
    }
}
```

> `Ellipsize` / `EllipsizeHeightLimit` are re-exported by libcosmic at
> `iced/core/src/widget/text.rs:37` (`pub use text::{Alignment, Ellipsize, …}`)
> and reach the app as `cosmic::iced::core::text::{Ellipsize, EllipsizeHeightLimit}`.
> **Verify the exact `EllipsizeHeightLimit` variant name at Task 1** against
> `iced/core/src/text.rs:199-230` before relying on it; if the variant set
> differs, use `Ellipsize::End(_)` with the actual single-line variant.

`plain_title()` is **kept** (used by the inactive-services rail) and `bounded_title()` is **added** for card rows.

### `src/app.rs` — explicit skin style

Replaces `src/app.rs:2166-2168`:

```rust
fn style(&self) -> Option<cosmic::iced::theme::Style> {
    if self.industrial() {
        // The skin owns its surface: system text/icon colour must never
        // leak into it (this is what `cosmic::applet::style()` would do).
        Some(cosmic::iced::theme::Style {
            background_color: cosmic::iced::Color::TRANSPARENT,
            text_color: COLOR_BRIGHT,
            icon_color: COLOR_KEY,
        })
    } else {
        Some(cosmic::applet::style())
    }
}
```

### `src/app.rs` — theme installation (skin → dark cosmic theme)

New `Message` variant (added alongside the other `Message` arms at `src/app.rs:963-1010`):

```rust
/// Install the skin's own dark theme over the system palette.
SetSkinTheme,
```

New `AppModel` method:

```rust
/// Build the skin's dark cosmic theme: the default dark COSMIC theme with
/// the user's accent applied to the accentable components.
fn skin_theme(&self) -> cosmic::Theme {
    let theme = cosmic::theme::Theme::dark();
    theme
}
```

Handled in `update()`:

```rust
Message::SetSkinTheme => {
    return cosmic::command::set_theme(self.skin_theme());
}
```

Issued from `init()` (after the config is loaded) and re-issued whenever the mode changes:

```rust
// In `Message::SetThemeMode(name)` arm, after persisting:
if self.industrial() {
    return cosmic::command::set_theme(self.skin_theme());
}
// Switching to System: hand the theme back to the system preference.
return cosmic::command::set_theme(cosmic::theme::system_preference());
```

> `cosmic::command::set_theme` is gated on libcosmic's `winit` feature, which
> **is** enabled in `Cargo.toml` (`features = [..., "winit"]`). The action is
> `Action::AppThemeChange`, handled at `libcosmic/src/app/cosmic.rs:911`.
> `cosmic::theme::system_preference()` exists at `libcosmic/src/theme/mod.rs:119`.

### `src/app.rs` — Preferences label column

Changed at `src/app.rs:1205-1290`: every `settings::item::builder(...).control(...)`
becomes `.flex_control(...)`. Verified against `libcosmic/src/widget/settings/item.rs:72-79`
(`min_item_width(200.0)`, `justify_content: SpaceBetween`) — this reserves a
readable label column instead of letting the control squeeze it.

Additionally, the Refresh Rate row (three buttons ≈ 300 px wide) is moved to
its own full-width row so it cannot compete with a label column:

```rust
// Refresh Rate: full-width band, label above the three keys.
pref_section = pref_section.add(
    settings::item::builder("Refresh Rate")
        .description("Background polling frequency for listening ports")
        .flex_control(
            widget::row::with_capacity(3)
                .spacing(6)
                .push(refresh_rate_key(1, fl!("refresh-mode-realtime"), self.config.refresh_interval(), self.accent()))
                .push(refresh_rate_key(2, fl!("refresh-mode-balanced"), self.config.refresh_interval(), self.accent()))
                .push(refresh_rate_key(10, fl!("refresh-mode-low"), self.config.refresh_interval(), self.accent())),
        ),
);
```

### `src/app.rs` — action key legibility

Changed at `src/app.rs:458-486` (`key_button_class`) and `src/app.rs:565-573`
(`key_style_flat`):

```rust
/// Rest state of a milled key: a faint plate + hairline border so the glyph
/// has an anchor, instead of floating ink on the card.
fn key_style(plate: cosmic::iced::Color, border: cosmic::iced::Color, ink: cosmic::iced::Color) -> cosmic::widget::button::Style {
    let mut style = cosmic::widget::button::Style::new();
    style.background = Some(cosmic::iced::Background::Color(plate));
    style.border_width = 1.0;
    style.border_color = border;
    style.border_radius = 4.0.into();
    style.icon_color = Some(ink);
    style.text_color = Some(ink);
    style
}
```

`key_button_class` mapping becomes:

| KeyColor | rest ink | rest plate | rest border |
|---|---|---|---|
| Neutral | `COLOR_INK_ACTIVE` (`#d4d4d8`) | `COLOR_SURFACE` | `COLOR_BORDER` |
| Blue | `LED_CYAN` | `COLOR_SURFACE` | `COLOR_BORDER` |
| Green | `accent.color` | `COLOR_SURFACE` | `COLOR_BORDER` |
| Danger | `LED_RED` | `COLOR_SURFACE` | `COLOR_BORDER` |

Hover/pressed states keep the existing `key_style(...)` mapping
(`src/app.rs:470-472`), which already lifts on hover.

### `src/app.rs` — title column bounds

Changed in `card_shell()` at `src/app.rs:830-849`: the title container gains an
explicit maximum width and the action cluster a fixed reserved width so a long
title can never be painted under the glyphs.

```rust
/// Width reserved for the action cluster (open · stop · expand · pin).
/// Fixed so the title column knows its true budget.
const ACTIONS_WIDTH: f32 = 4.0 * 28.0 + 3.0 * 6.0 + 28.0; // 4 keys + pin + gaps
```

```rust
let title_row = widget::row::with_capacity(2)
    .align_y(cosmic::iced::Alignment::Center)
    .width(Length::Fill)
    .push(
        widget::container(title_left)
            .width(Length::Fill)      // bounded: may shrink, never overflow
            .clip(true),
    )
    .push(
        widget::container(actions_right)
            .width(Length::Fixed(ACTIONS_WIDTH))
            .clip(false),
    );
```

Combined with `bounded_title()` using `Ellipsize::End(single-line)`, a long
service name now truncates with `…` inside its own column and never reaches
the buttons.

### `src/app.rs` — Listening Ports rail default

Changed in `init()` at `src/app.rs:1029-1040`, plus a v1→v2 migration step:

```rust
let (config, config_ctx) = match cosmic::cosmic_config::Config::new(APP_ID, Config::VERSION) {
    Ok(ctx) => match Config::get_entry(&ctx) {
        Ok(config) => (config, Some(ctx)),
        Err((_errors, config)) => (config, Some(ctx)),
    },
    Err(_) => (Config::default(), None),
};
```

Becomes a migration-aware load (v1 → v2), with the **Listening Ports rail
starting collapsed**:

```rust
// v1 shipped with the Listening Ports rail open. v2 starts it collapsed,
// so a stale v1 `show_ports = true` is reset once — the toggle stays.
let mut config = config;
if config.show_ports {
    config.show_ports = false;
}
```

and the same guard re-applied after `Config::get_entry` returns an entry for
version 1 (`Config::get_entry` for `Config::VERSION`); the persisted value is
then written back with `ctx.set("show_ports", false)` so the reset sticks.

### `src/app.rs` — theme-mode persistence

Changed at `src/app.rs:2055-2063` so the skin is written canonically:

```rust
Message::SetThemeMode(name) => {
    let normalised = if name.trim().eq_ignore_ascii_case("system") {
        "system".to_string()
    } else {
        "industrial".to_string()
    };
    self.config.theme_mode = normalised;
    if let Some(ctx) = &self.config_ctx {
        let _ = ctx.set("theme_mode", self.config.theme_mode.clone());
    }
    // re-apply / release the theme at the same time
    if self.industrial() {
        return cosmic::command::set_theme(self.skin_theme());
    }
    return cosmic::command::set_theme(cosmic::theme::system_preference());
}
```

## D2. Execution Logic

| Event | Skin active? | Behaviour |
|---|---|---|
| App start, `theme_mode` unset/empty | **yes** (skin) | `style()` ⇒ skin colours; `set_theme(dark)` issued from `init()`; chassis paints `#14161b`; Listening Ports rail **collapsed** |
| App start, `theme_mode = "industrial"` | yes | identical to above — legacy value keeps the skin |
| App start, `theme_mode = "system"` | no | `style()` ⇒ `cosmic::applet::style()`; `set_theme(system_preference())`; COSMIC containers; **rail still collapsed** (independent of theme) |
| User picks "Industrial" in Preferences | yes | `theme_mode` persisted `industrial`; `set_theme(dark)`; UI re-renders with skin |
| User picks "System" in Preferences | no | `theme_mode` persisted `system`; `set_theme(system_preference())`; COSMIC look returns |
| Long service name | n/a | title ellipsises at the end inside its bounded column; action cluster never overlapped |
| Listening Ports rail toggle | n/a | show/hide still works exactly as before; only the **initial** state is collapsed |

**Error handling.** `cosmic::command::set_theme` returns a `Task`, never a
`Result`; there is no failure path to handle. The existing `Message::Open`
error path (`src/app.rs:2003-2006`) is untouched. The existing config-load
`Err((_errors, config))` arm keeps using the returned config (which carries
the v2 defaults) — errors are non-fatal by design here.

**Edge cases.**
- `theme_mode` contains garbage (e.g. `"Dark"`) ⇒ `use_industrial()` returns **true** (only the exact string `"system"` opts out) — the skin is the safe default.
- `theme_mode` has whitespace (`" system "`) ⇒ `.trim()` handles it.
- A persisted v1 `show_ports = true` ⇒ reset to `false` once, then the user's own toggle choices are respected thereafter.
- Switching theme while the popup is open ⇒ the re-render picks up the new `style()` immediately; the popup's transparent background means no flash of the old chassis.

## D3. File-by-File Ledger

| Path | Action | Change |
|---|---|---|
| `src/config.rs` | **MODIFY** | `#[version = 1]` → `#[version = 2]`; `use_industrial()` → skin-by-default (`!eq_ignore_ascii_case("system")`); **new** `theme_mode_name()`; **new** `migrate_from_v1(&Config) -> Config`; doc comments on `theme_mode` / `show_ports` updated to state the new defaults; **new** unit tests: `use_industrial()` is true for `""`, `"industrial"`, `"Dark"` and false for `"system"`; `migrate_from_v1` forces `show_ports == false` and preserves every other field. |
| `src/app.rs` | **MODIFY** | (a) `fn style()` at 2166 ⇒ explicit skin style; (b) **new** `Message::SetSkinTheme`; (c) **new** `fn skin_theme(&self) -> cosmic::Theme`; (d) `init()` ⇒ issue `set_theme`, and apply the v1→v2 `show_ports` reset; (e) `Message::SetThemeMode` ⇒ persist canonically + swap theme; (f) `Message::SetSkinTheme` ⇒ new arm; (g) **new** `fn bounded_title()` + `const COLOR_INK_ACTIVE` + `const ACTIONS_WIDTH`; (h) `plain_title()` ⇒ keep; card rows ⇒ use `bounded_title()`; (i) `card_shell()` ⇒ bounded title column + fixed action cluster width; (j) `key_button_class()` / `key_style()` ⇒ rest plate + brighter ink; (k) Preferences `settings::item` rows ⇒ `.flex_control()` (and Refresh Rate onto its own band); (l) `use_industrial()` call sites unchanged (`src/app.rs:3052`) but now resolve skin-first. |
| `resources/app.metainfo.xml` | **MODIFY** | Description paragraph rewritten: replace *"It follows the system COSMIC theme by default, with an optional legacy industrial dark look available in Preferences."* with a statement that the applet ships **skinned** with an optional *System* theme in Preferences. Add a `<release version="0.2.0" date="…">` entry describing the skin-first default, the collapsed Listening Ports rail, and the layout/legibility fixes. |
| `README.md` | **MODIFY** | The "What it does" bullet list gains one line stating that the applet ships with its own dark instrument skin and that a *System* theme option exists in Preferences. |
| `src/app.rs` (tests) | **MODIFY** | **new** tests in the existing `mod tests` (line 3033): skin resolution matrix; `bounded_title`/`card_shell` compile-level smoke; theme-mode normalisation. |
| `plans/2026-10-26_105349-skin-self-contained-industrial-default.md` | **MODIFY** | Execution ledger appended as work progresses. |
| `WORKLOG.md` / `worklog.db` | **MODIFY (via `logwork`)** | Every action logged via the `logwork` CLI, per the workspace `AGENTS.md`. |

**No files are deleted. No message variants, no service capabilities, no port actions, no preferences, and no theme option are removed.**

---

# E. Option 2 Execution Summary

**Files changed:** `src/config.rs`, `src/app.rs` only. (`resources/app.metainfo.xml` and `README.md` would *still* need a one-line truth fix, otherwise the listing contradicts the app — that edit is not optional under either option.)

**Structural differences vs. Option 1:**

| Concern | Option 1 | Option 2 |
|---|---|---|
| `use_industrial()` | skin-first: `!eq_ignore_ascii_case("system")` | identical (skin-first) |
| `style()` | returns an explicit skin `theme::Style` | **unchanged** — keeps `cosmic::applet::style()`, so system text/icon colour still leaks into the skin |
| Runtime theme | `cosmic::command::set_theme(dark)` installs a dark cosmic theme; skin is identical under a light system theme | not installed — `Container::Card`, `Button::Suggested`, togglers and text inputs still follow the system palette, so the skin is **partially** applied on a light system |
| Preferences | `.flex_control()` + Refresh Rate on its own band | one explicit min-width on the label column |
| Action keys | rest plate + brighter ink | brighter ink only (no rest plate) |
| Long titles | `Ellipsize::End` + bounded column + fixed action-cluster width | `Ellipsize::End` only |
| Listening Ports | default collapsed via v1→v2 migration | `show_ports = false` forced at startup in `init()` (no version bump — the persisted value is silently overwritten on **every** launch, so the rail can never be opened again ⇒ **this variant is unacceptable**; use the v2 migration instead) |
| New messages | `Message::SetSkinTheme` | none |

**Trade-off.** Option 2 lands in one edit pass and never introduces a runtime
theme swap, so it has the lowest chance of a regression. It does **not**
deliver the core requirement — "it MUST be skinned" — on a light system theme:
the skin would be visible but the COSMIC-natively-themed components inside it
would not match, which is precisely the "partially skinned / no agent can
comprehend" state reported. Option 2 is only correct if the deadline forbids
the theme swap entirely.

**Note on the `show_ports` variant in Option 2:** forcing `show_ports = false`
at every startup would break the toggle (the rail could never be opened). The
correct lean implementation is a **stored one-shot flag** or the same v2
migration as Option 1 — the migration is one line in Option 1, so Option 2
should reuse it.

---

# F. Discrete Task Checklist (Phase-Gated)

## Phase 0 — Baseline Gate

- [ ] **GATE: Run §G verification commands against the unmodified tree. If any fail unexpectedly (excluding documented baseline failures from §B), set status BLOCKED, record output in execution ledger, and halt before modifying files.**

## Phase 1 — Skin resolution (config layer)

- [ ] `src/config.rs`: bump `#[version = 1]` → `#[version = 2]`.
- [ ] `src/config.rs`: rewrite `use_industrial()` to the skin-first rule and add `theme_mode_name()`.
- [ ] `src/config.rs`: add `migrate_from_v1()` resetting `show_ports` to `false`.
- [ ] `src/config.rs`: add unit tests — skin-resolution matrix + migration field preservation.
- [ ] **GATE: `cargo test --locked --offline 2>&1 || cargo test --locked` — the `config.rs` tests must pass (all 7 pre-existing tests plus the new ones).**

## Phase 2 — Skin is self-contained (app layer)

- [ ] `src/app.rs`: add `const COLOR_INK_ACTIVE` and `const ACTIONS_WIDTH`.
- [ ] `src/app.rs`: add `fn bounded_title()` using `Ellipsize::End(single-line)`; keep `plain_title()`.
- [ ] `src/app.rs`: rewrite `fn style()` to return the explicit skin style when `self.industrial()`.
- [ ] `src/app.rs`: add `fn skin_theme(&self) -> cosmic::Theme` and `Message::SetSkinTheme`; handle the new arm.
- [ ] `src/app.rs`: in `init()`, issue the skin theme and apply the v1→v2 `show_ports` reset; persist the reset.
- [ ] `src/app.rs`: in `Message::SetThemeMode`, persist canonically and swap the theme in both directions.
- [ ] **GATE: `cargo build --locked` — must compile clean; `cargo clippy --all-features --locked -- -W clippy::pedantic` — zero new warnings.**

## Phase 3 — Layout and legibility

- [ ] `src/app.rs`: switch the Preferences `settings::item` rows to `.flex_control(...)`; move Refresh Rate to its own full-width band.
- [ ] `src/app.rs`: `card_shell()` — bound the title column, reserve a fixed action-cluster width.
- [ ] `src/app.rs`: card rows use `bounded_title()` instead of `plain_title()`.
- [ ] `src/app.rs`: `key_button_class()` / `key_style()` — rest plate + brighter ink for all four key colours.
- [ ] `src/app.rs`: add tests for the title-ellipsis and key-style helpers.
- [ ] **GATE: `cargo test --locked` — all tests pass; `cargo clippy --all-features --locked -- -W clippy::pedantic` — zero new warnings.**

## Phase 4 — Listing truth

- [ ] `resources/app.metainfo.xml`: rewrite the description paragraph to state the skin-first default and the System option.
- [ ] `resources/app.metainfo.xml`: add a `<release version="0.2.0">` entry.
- [ ] `README.md`: add one line describing the shipped skin + System option.
- [ ] **GATE: `cargo build --locked --release` — must succeed; the metainfo must parse (verified by `xdgen` in `build.rs`, which runs during the build).**

## Phase 5 — Visual verification

- [ ] Deploy the release binary to `~/.local/bin/lazy-ports` and restart `cosmic-panel` (the documented live-deploy path in `WORKLOG.md`).
- [ ] Capture the popup under the skin: Preferences (`flex_control` label column no longer a ribbon), the ports list (action keys legible at rest, long names ellipsised, no overlap with the buttons), and the Listening Ports rail **collapsed** on first open.
- [ ] Repeat the capture **with the COSMIC system theme forced to Light** to prove the skin is system-theme-independent.
- [ ] **GATE: all four observations above confirmed — label column readable, action glyphs visible at rest, long names ellipsised and not overlapping, Listening Ports rail collapsed on a fresh start.**

## Phase 6 — Release & upload

- [ ] Resolve **O1**: create `github.com/hayron/lazy-ports` under the authenticated account; if it cannot be created, use `GYCinc/lazy-ports` and update `resources/app.metainfo.xml` (`<url>`, icon URL) and `Cargo.toml` `repository` to the real location.
- [ ] Configure the git remote and push `main`:
      `git remote add origin <resolved-url>` → `git push -u origin main`
- [ ] Tag and push the release: `git tag -a v0.2.0 -m ''` → `git push origin v0.2.0`
- [ ] Create the GitHub release with the built binary + the `.desktop` and metainfo:
      `gh release create v0.2.0 target/release/lazy-ports resources/app.desktop resources/app.metainfo.xml --title "Lazy Ports 0.2.0" --notes "…"`
- [ ] **GATE: `git ls-remote origin` lists `main` and `v0.2.0`; `gh release view v0.2.0` lists three assets; `gh api repos/<owner>/lazy-ports` returns 200 for the pushed repo.**

## Phase 7 — Close-out

- [ ] Run the full verification from §G and confirm the done criteria.
- [ ] Append the execution ledger (below) with final status `EXECUTED`.
- [ ] Log every action through the `logwork` CLI into `WORKLOG.md` / `worklog.db`.

---

# G. Verification, Regression & Rollback

## Verification commands (from §L — `just` is NOT installed; use raw `cargo`)

```sh
# Build (the first run needs the network for `combine 4.6.8`; Cargo.lock pins
# 4.6.8 while only 4.6.7 is in the local registry cache)
cd /home/hayron/MyProjects/lazy-ports
cargo build --locked

# Tests (7 pre-existing: 2 app.rs, 2 config.rs, 1 ports.rs, 2 services.rs)
cargo test --locked

# Lint — mirrors the `just check` recipe
cargo clippy --all-features --locked -- -W clippy::pedantic

# Release build (also exercises xdgen → validates the metainfo XML)
cargo build --locked --release

# Install (the `just install` recipe, inlined because `just` is absent)
install -Dm0755 target/release/lazy-ports ~/.local/bin/lazy-ports
install -Dm0644 resources/app.desktop ~/.local/share/applications/io.github.hayron.lazy-ports.desktop
install -Dm0644 resources/app.metainfo.xml ~/.local/share/metainfo/io.github.hayron.lazy-ports.metainfo.xml
install -Dm0644 resources/icons/hicolor/256x256/apps/io.github.hayron.lazy-ports.png ~/.local/share/icons/hicolor/256x256/apps/io.github.hayron.lazy-ports.png
install -Dm0644 resources/icons/hicolor/128x128/apps/io.github.hayron.lazy-ports.png ~/.local/share/icons/hicolor/128x128/apps/io.github.hayron.lazy-ports.png
install -Dm0644 resources/icons/hicolor/scalable/apps/io.github.hayron.lazy-ports-symbolic.svg ~/.local/share/icons/hicolor/scalable/apps/io.github.hayron.lazy-ports-symbolic.svg

# Live reload the panel applet
systemctl --user restart cosmic-panel    # then re-add the applet if needed
```

**Documented baseline conditions (not failures):**
- `cargo build --offline` fails today: `combine v4.6.8` is not in the local registry cache. The first build **must** be online.
- `cargo metadata --offline` fails for the same reason.
- `just` is not installed ⇒ `just check` / `just install` are unavailable; use the inline equivalents above.
- The live config `~/.config/cosmic/io.github.hayron.lazy-ports/v1/show_ports` is `true` ⇒ the rail will start **open** until Phase 2's migration is in place. Expected.

## Manual scenarios

| # | Input | Expected result |
|---|---|---|
| 1 | Fresh install, no `theme_mode` in the config | Applet opens **skinned** (dark chassis, skin text/icon colour). **Boundary:** `theme_mode` absent entirely ⇒ skin. |
| 2 | Fresh install under a **Light** COSMIC system theme | The skin renders identically to the dark-system case — chassis `#14161b`, skin text/icon colour, no white COSMIC cards bleeding in. **This is the core acceptance test for "MUST be skinned".** |
| 3 | `theme_mode = "system"` | COSMIC-native look returns; the Preferences rows use `Container::Card`; the theme switch is visibly immediate, no restart. |
| 4 | `theme_mode = "industrial"` (legacy persisted value) | Skin — identical to case 1. **Boundary:** no user loses their existing look. |
| 5 | `theme_mode = "Dark"` (garbage value) | Skin (only the exact string `"system"` opts out). **Boundary/adversarial.** |
| 6 | Open Preferences | Every row shows a readable title + description on the **left**, control on the right; the Refresh Rate band shows its three keys on one line beneath the title; the label column is **not** a narrow vertical ribbon. |
| 7 | Port list, ordinary rows | Action glyphs (open / stop / expand / pin) are **clearly visible at rest** without hover — each sits on its own hairline-bordered plate. |
| 8 | Port list, a service named `pipewire_preply_webrtc_recording_x` longer than the column | The name truncates with a trailing `…` **inside its column**; it does **not** overlap or touch the action keys; the port number and PID stay on the endpoint line below. **Adversarial/boundary.** |
| 9 | First open after upgrade from the current live config (which has `show_ports = true`) | The Listening Ports rail is **collapsed**; the count chip still shows the port total; clicking "Show" opens it and the choice persists across restarts. |
| 10 | Restart the applet after expanding the rail in scenario 9 | The rail stays **expanded** — the v1→v2 reset fires exactly once. **Boundary/adversarial.** |
| 11 | Toggle System → Industrial in Preferences without restarting | The skin appears immediately: chassis repaints, text/icon colour swaps, no stale colours, no crash. |
| 12 | Roll the popup while the skin is active | The chassis keeps its 12 px corner radius and hairline edge; no white flash; the popup stays transparent outside the chassis. |

## Rollback

Per-phase revert, in reverse order. Every phase is a commit, so each reverts cleanly.

```sh
# Phase 6 (upload) — delete the remote tag + release, keep the local commits
git push origin --delete v0.2.0
gh release delete v0.2.0 --yes
# The remote branch can stay; nothing on the user's machine depends on it.

# Phase 5 (visual) — restore the previous binary
# (no repo change; re-install from the previous release build)

# Phase 4 (listing)
git revert <phase-4-commit>

# Phase 3 (layout)
git revert <phase-3-commit>

# Phase 2 (skin layer)
git revert <phase-2-commit>

# Phase 1 (config)
git revert <phase-1-commit>

# Full rollback to the pre-plan state
git reset --hard 15153f3
```

**Non-reversible / manual-unwind items:**
- **Config version bump (`#[version]` 1 → 2).** `cosmic-config` stores per-version key files under `~/.config/cosmic/io.github.hayron.lazy-ports/v1/` and `/v2/`. Rolling the binary back to v1 means the v2 keys are ignored and v1 is read again — **no data loss**; `services_json` stays in v1 and is still read. **However**, `services_json` is re-persisted under v2 once the new binary writes, so after a rollback the v1 `services_json` may be stale relative to v2. Manual unwind if needed:
  `cp ~/.config/cosmic/io.github.hayron.lazy-ports/v2/services_json ~/.config/cosmic/io.github.hayron.lazy-ports/v1/services_json`
  (That path is only reachable if the user edited the service list on the v2 build.)
- **Pinned-skill state change:** `Message::SetSkinTheme` writes to the **process-global** `cosmic::theme::THEME` (`libcosmic/src/theme/mod.rs:47`). It affects only this process (the applet), never `cosmic-panel`. A crash mid-swap leaves the process in whichever theme was last set — the next launch re-derives it from the config, so there is no persistent damage.
- **Published GitHub release:** `gh release delete v0.2.0 --yes` removes the assets; the tag must also be deleted remotely (`git push origin --delete v0.2.0`). Both are re-creatable, so this is classified reversible.

**Baseline failure risk:** `cargo build --locked` pulls `combine 4.6.8` from crates.io. If crates.io is unreachable at execution time, the build is blocked regardless of this plan — resolve by allowing network for the first build.

---

# H. Execution Ledger

| Timestamp | Actor | Old Status | New Status | Notes / Blockers / Learnings |
| :--- | :--- | :--- | :--- | :--- |
| 2026-10-26 10:53 | @PlanAgent | N/A | PROPOSED | Initial plan drafted. Discovery log K/W/L/H recorded. Baseline commit `15153f3`. Key findings: (1) `src/config.rs:55` empty ⇒ system is the cause of the generic look; (2) `src/app.rs:2166` `cosmic::applet::style()` is the system-colour leak; (3) `settings::item` label `Fill` vs control `Shrink` (`libcosmic/src/widget/settings/item.rs:39-45`) reproduces the squeezed Preferences ribbon — `flex_control()` (`:72-79`) is the verified fix; (4) `Text::ellipsize` (`iced/core/src/widget/text.rs:166`) is the verified fix for long-name overflow; (5) the Listening Ports rail default is a **stale persisted** `show_ports=true` in the live config, not the derive default — needs a v1→v2 migration, not a startup force; (6) `cosmic::command::set_theme` is available (`winit` feature enabled) so the skin can be made system-theme-independent; (7) upload blocker: `github.com/hayron/lazy-ports` does not exist and no git remote is configured — `GYCinc/loPee` is the only existing repo; (8) `just` is not installed and `combine 4.6.8` is not cached ⇒ raw `cargo` commands + one online fetch. |
| | | | | **Open O1:** upload repo name (`hayron/lazy-ports` vs `GYCinc/lazy-ports`) — resolved at Phase 6, affects only remote URL + metainfo URL. **Assumptions A1–A5** recorded in §B. Confidence 88%. |
