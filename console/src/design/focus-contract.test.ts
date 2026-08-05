/*
 * The focus ring, as a contract rather than a promise.
 *
 * The console reached 107 hand-rolled controls with nine focus declarations
 * between them precisely because nothing failed when someone left the ring
 * out. Migrating them onto Button/Input fixes today; this fixes tomorrow.
 *
 * It asserts three things a reviewer cannot reliably eyeball:
 *   1. the mechanism is still wired (the token, the class, the unlayered rule);
 *   2. no control on a shipped route suppresses the ring with an inline
 *      `outline`, which is the one thing that still beats it;
 *   3. every native <button>/<input>/<textarea>/<select> left outside the
 *      primitives is on the allowlist below, WITH a reason, and carries FOCUS
 *      or is a genuinely non-interactive element.
 *
 * If you are here because this failed: the fix is almost never to widen the
 * allowlist. It is to use Button/Input, or to add the FOCUS class.
 */

import fs from "node:fs";
import path from "node:path";

import { describe, expect, it } from "vitest";

import { FOCUS, FOCUS_RING } from "./theme";

const ROOT = path.resolve(__dirname, "../..");

/** The routes this repo actually ships: /console, /login, /signup, /demo. */
const SHIPPED = [
  "app/console",
  "app/demo",
  "app/login",
  "app/signup",
  "src/components",
  "src/docs",
  "src/divergence",
  "src/observatory",
  "src/health",
  "src/ops",
  "src/design",
];
const SHIPPED_FILES = ["app/chrome.tsx"];

/**
 * Native controls that stay native, each because the primitive would cost more
 * than it gives. Every one of these still gets a ring — the first three carry
 * FOCUS explicitly, the rest are covered by the globals.css baseline.
 */
const ALLOWED_NATIVE: Record<string, string> = {
  "src/components/ui/Button.tsx": "the primitive itself",
  "src/components/ui/Input.tsx": "the primitives themselves",
  "app/console/modules/memories/Archive.tsx":
    "table rows carry the header's 5-column grid, which row's flex would collapse; " +
    "the as-of scrubber is a range slider, not a text box",
  "app/console/modules/memories/ArchiveSearch.tsx":
    "owns its own border, focus-within tint and inline counter — Input would nest a second one",
  "app/console/modules/graph/views/DepthOfFieldView.tsx":
    "canonical cards are stacked blocks, not a single-line row",
  "app/login/page.tsx": "<input type=hidden> — not an interactive control",
};

function walk(dir: string, out: string[] = []): string[] {
  if (!fs.existsSync(dir)) return out;
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) walk(p, out);
    else if (e.name.endsWith(".tsx")) out.push(p);
  }
  return out;
}

/** Comments discuss `outline: none` at length; they are not code. */
const stripComments = (s: string) =>
  s
    .replace(/\{\/\*[\s\S]*?\*\/\}/g, "")
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/(^|[^:])\/\/[^\n]*/g, "$1");

const shippedFiles = [
  ...SHIPPED_FILES.map((f) => path.join(ROOT, f)),
  ...SHIPPED.flatMap((d) => walk(path.join(ROOT, d))),
];
const rel = (p: string) => path.relative(ROOT, p).split(path.sep).join("/");

describe("the mechanism is wired", () => {
  const css = fs.readFileSync(path.join(ROOT, "app/globals.css"), "utf8");

  it("defines --focus-ring, and theme.ts agrees with it", () => {
    const declared = css.match(/--focus-ring:\s*([^;]+);/)?.[1].trim();
    expect(declared).toBeDefined();
    // Same colour on both sides of the JS/CSS boundary. They drift silently
    // otherwise: nothing renders wrong, the two just stop being one token.
    expect(declared!.replace(/\s+/g, "")).toBe(FOCUS_RING.replace(/\s+/g, ""));
  });

  it("applies it through the class theme.ts exports", () => {
    expect(css).toContain(`.${FOCUS}:focus-visible`);
  });

  it("keeps the rule OUTSIDE a cascade layer", () => {
    // This is load-bearing. Tailwind emits `focus:outline-none` into
    // @layer utilities, and unlayered rules outrank layered ones regardless of
    // specificity — that is the only reason the ring wins without every
    // existing call site being rewritten first. Wrap this in @layer and the
    // ring silently stops applying wherever outline-none survives.
    const rule = css.indexOf(`.${FOCUS}:focus-visible`);
    const before = css.slice(0, rule);
    const opened = (before.match(/@layer[^;]*\{/g) ?? []).length;
    expect(opened).toBe(0);
  });

  it("covers the standard interactive elements", () => {
    for (const el of ["a", "button", "input", "select", "textarea"]) {
      expect(css).toMatch(new RegExp(`[(,\\s]${el}[,)\\s]`));
    }
  });
});

describe("nothing on a shipped route defeats it", () => {
  it("no inline outline on any element", () => {
    // An inline style outranks even the unlayered rule. The dispute band strip
    // set `outline: on ? "1px solid" : "none"`, so every UNSELECTED segment
    // carried an explicit outline:none and was invisible to the keyboard. Use
    // `box-shadow: inset 0 0 0 1px` for a static edge instead.
    const offenders = shippedFiles.filter((f) =>
      /\boutline:\s*(?!undefined)/.test(stripComments(fs.readFileSync(f, "utf8"))),
    );
    expect(offenders.map(rel)).toEqual([]);
  });
});

describe("every native control is accounted for", () => {
  it("only the allowlisted files hand-roll a control", () => {
    const found = shippedFiles.filter((f) =>
      /<(button|input|textarea|select)[\s>]/.test(
        stripComments(fs.readFileSync(f, "utf8")),
      ),
    );
    expect(found.map(rel).sort()).toEqual(Object.keys(ALLOWED_NATIVE).sort());
  });

  it("each allowlist entry states why", () => {
    for (const [file, reason] of Object.entries(ALLOWED_NATIVE)) {
      expect(fs.existsSync(path.join(ROOT, file)), file).toBe(true);
      expect(reason.length, file).toBeGreaterThan(15);
    }
  });

  it("the hand-rolled buttons still carry the ring", () => {
    // Buttons need the explicit class: unlike an <input>, a <button> that has
    // been given a custom layout is easy to mistake for chrome.
    for (const file of [
      "app/console/modules/memories/Archive.tsx",
      "app/console/modules/graph/views/DepthOfFieldView.tsx",
    ]) {
      // Comments in these two files literally say "stays a plain <button>",
      // so they must be stripped or the prose fails the assertion.
      const src = stripComments(fs.readFileSync(path.join(ROOT, file), "utf8"));
      // A window rather than "up to the closing >": an arrow function in an
      // onClick contains a `>`, so a lazy match to the first one stops inside
      // the handler and never reaches className.
      for (const m of src.matchAll(/<button\b/g)) {
        const tag = src.slice(m.index, m.index + 700);
        expect(tag, `${file}: a hand-rolled <button> without FOCUS`).toMatch(
          /FOCUS/,
        );
      }
    }
  });
});
