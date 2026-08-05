import { describe, expect, it } from "vitest";

import { FOCUS, GOLD, INK_DIM } from "@/design/theme";

import {
  controlClass,
  controlStyle,
  fieldClass,
  fieldStyle,
  type ControlVariant,
} from "./control-styles";

const VARIANTS: ControlVariant[] = ["solid", "pill", "quiet", "icon", "row"];

describe("focus ring", () => {
  // This is the contract the primitives exist for. If a variant can be built
  // without the ring, the console has regressed to the state that made this
  // work necessary: 107 controls, 9 focus declarations.
  it.each(VARIANTS)("%s carries the focus ring", (variant) => {
    expect(controlClass(variant, "sm").split(" ")).toContain(FOCUS);
  });

  it.each(["xs", "sm", "md"] as const)("fields carry it at size %s", (size) => {
    expect(fieldClass(size).split(" ")).toContain(FOCUS);
  });

  it("survives a caller's own className", () => {
    expect(controlClass("pill", "sm", "ml-auto shrink-0").split(" ")).toContain(
      FOCUS,
    );
  });

  it("never emits outline-none, which would suppress it", () => {
    for (const variant of VARIANTS) {
      expect(controlClass(variant, "sm")).not.toContain("outline-none");
    }
    expect(fieldClass("sm")).not.toContain("outline-none");
  });
});

describe("accent", () => {
  it("reaches border and text on a pill", () => {
    const s = controlStyle("pill", GOLD);
    expect(s.color).toBe(GOLD);
    // withAlpha, not string concatenation — GOLD is hsla, so appending a hex
    // alpha would emit invalid CSS the browser silently drops.
    expect(s.borderColor).toMatch(/^hsla\(/);
    expect(s.borderColor).not.toContain(")0");
  });

  it("fills a solid button and darkens its ink", () => {
    const s = controlStyle("solid", GOLD);
    expect(s.background).toBe(GOLD);
    expect(s.color).toBe("#08070c");
    expect(s.boxShadow).toContain("hsla(");
  });

  it("falls back to dim ink with no accent", () => {
    expect(controlStyle("pill", undefined).color).toBe(INK_DIM);
    expect(controlStyle("quiet", undefined).color).toBe(INK_DIM);
  });

  it("brings the tint forward when selected", () => {
    const off = controlStyle("pill", GOLD, false);
    const on = controlStyle("pill", GOLD, true);
    expect(off.background).toBeUndefined();
    expect(on.background).toBeDefined();
  });
});

describe("width", () => {
  // Tailwind resolves w-full vs w-72 by stylesheet order, not className order,
  // so the primitive must not bake in a width the caller means to override.
  it("is full by default and omitted when the caller owns it", () => {
    expect(fieldClass("sm").split(" ")).toContain("w-full");
    expect(fieldClass("sm", "w-72", false).split(" ")).not.toContain("w-full");
  });
});

describe("invalid fields", () => {
  it("switch the border to the contradiction tone", () => {
    expect(fieldStyle(false).borderColor).toContain("233,237,255");
    expect(fieldStyle(true).borderColor).toContain("255,93,162");
  });
});
