# Console design system

The console's visual language, and the decisions behind it that are easy to
mistake for gaps.

Source of truth for values: [`console/src/design/theme.ts`](../console/src/design/theme.ts).
Source of truth for control shape: [`console/src/components/ui/`](../console/src/components/ui/).

---

## 1. Dark-only is a decision, not a missing feature

**There is no light theme, and one should not be added.**

The console is a fixed art direction. The whole identity is the EEG band
system — every module is tuned to a band (delta/theta/alpha/beta/gamma) and
takes its accent hue from it, with gamma, the binding band, as the brand
colour. Those hues are chosen to sit at a specific luminance against a near
-black ground (`--bg: #08070c`); on white they invert into a set of muddy
mid-tones that carry none of the same meaning. A light theme would not be a
port of this design, it would be a second design.

The audience makes the trade cheap: the console is an operator instrument,
used the way a terminal or a DAW is used, and instrument software overwhelmingly
lives in the dark. So the theme layer deliberately spends its complexity budget
on the band system instead of on a second palette, and every colour in
`theme.ts` is a literal hex/hsl rather than a themeable variable pair.

Consequences that follow on purpose:

- No `prefers-color-scheme` handling, no `[data-theme]` selectors, no
  light/dark token pairs.
- `theme.ts` colours are literals. That is not drift.
- Contrast is checked against the dark ground only.

If this is ever revisited, it is a redesign with its own brief — not a
find-and-replace over the token file.

## 2. Focus visibility

Every interactive control draws a visible ring on keyboard focus. This is
enforced in two places, both in [`console/app/globals.css`](../console/app/globals.css),
and both deliberately **outside any `@layer`**.

```css
:where(a, button, input, select, textarea, summary, [tabindex]):focus-visible,
.focus-ring:focus-visible { outline: 2px solid var(--focus-ring); outline-offset: 2px; }
```

Why unlayered: Tailwind 4 emits all of its utilities into cascade layers, and
unlayered declarations outrank layered ones **regardless of specificity**. The
console had `focus:outline-none` scattered through its inputs — a layered
override would have had to out-specify each one; an unlayered rule simply wins.
Verified against the compiled stylesheet: `.focus\:outline-none` is emitted
inside `@layer utilities`, the focus rule after it at top level.

The `:where()` baseline is the safety net — it covers controls nobody has
migrated yet, at zero specificity so a component with a real reason can still
override it (the recharts block does). `.focus-ring` is the explicit opt-in
that the shared primitives apply, and it carries real specificity.

**An inline `outline` still beats both.** The dispute band strip set
`outline: on ? "1px solid" : "none"` inline, which meant every unselected
segment carried an explicit `outline: none`. If you need a static edge, use
`box-shadow: inset 0 0 0 1px`, and leave `outline` to the keyboard.

## 3. The primitives

| | |
|---|---|
| `ui/Button.tsx` | hook-free and directive-free, so server components can render it |
| `ui/Input.tsx` | `Input` / `Textarea` / `Select`; `"use client"` because `useId` wires label→field |
| `ui/control-styles.ts` | the pure variant/size/accent logic, and the only part with tests |

Variants are named after what the console does, not after a generic taxonomy:

- `solid` — the one loud affordance on a page. At most one per view.
- `pill` — the workhorse. Bordered capsule, accent-tinted.
- `quiet` — text-only, and also the right answer for a *shaped* control that
  is neither a capsule nor a single-line row: it imposes no geometry and
  contributes only the ring and the cursor/disabled contract.
- `icon` — square hit area around a glyph. Needs an `aria-label`.
- `row` — full-width, left-aligned, single-line selectable list row.

`accent` is a theme colour, not a semantic name (`intent="danger"`). The band
system already assigns meaning per module, and a fixed semantic palette would
fight it: the constructive answer is gamma on the review queue and alpha on
governance, and both are "primary" on their own surface.

### Two cascade traps, both measured

1. **An inline style beats every `hover:` utility.** `controlStyle` returns an
   inline `color`/`borderColor`, so a `hover:text-*` on the same element is
   dead. If a control recolours on hover, hand the colour back with
   `style={{ color: undefined }}` and keep the pair in `className`. (The
   primitives' own hover is `brightness`, a filter, for exactly this reason.)
2. **Tailwind resolves competing utilities by stylesheet order, not className
   order.** Measured against the compiled sheet:
   - radius and type scale are safe — `rounded-full` is emitted *before*
     `rounded-md`, `text-sm` before `text-xs`, so overrides win;
   - **padding is not** — the scale ascends (`px-2` … `px-5`), so an override
     only wins when it is *larger*. Pass `unpadded` and supply your own.
   - width is not — hence `fullWidth` on the field primitives.

## 4. Colour

Import the token from `theme.ts`. The CSS custom properties in `globals.css`
(`--bg --ink --gold --magenta --amber --surface-solid`) exist for the one case
a JS import cannot reach: a hex inside a Tailwind arbitrary value
(`text-[#e9edff]/55`). Same values, one definition.

`withAlpha()` — never append a hex alpha to an interpolated colour. Half this
palette comes from `band()` and is an `hsla()` string, so `GOLD + "14"` emits
CSS the browser silently drops, and the bug reads as a design choice. The
docblock in `theme.ts` has the incident.

## 5. Known debt

- **~13 hex literals remain in the shipped-route tree**, all genuine one-offs
  with no token: the mermaid diagram theme, the dispute decay gradient
  (`#ff8a5d`/`#7fa8f5`/`#c9b6ff`), and three panel shades a hair off `PANEL`.
  The marketing/KB/pitch surfaces (`src/home`, `src/kb`, `src/pitch`,
  `src/library`) were out of scope and hold the rest.
- **Four controls stay native `<button>`/`<input>`** where the primitive would
  cost more than it gives — the as-of range slider, the archive table rows
  (a five-column grid `row`'s flex would collapse), the depth-of-field cards
  (stacked blocks), and the archive search field (it owns its own border and
  focus-within chrome). Each carries the `FOCUS` class instead.
- **`src/home` and `src/pitch` still hand-roll their CTAs.** They are the
  marketing surfaces, not the console, and a copy rewrite was in flight.
