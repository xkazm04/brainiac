/*
 * A styled section break: a hairline that carries the brand's band glyph.
 * Extracted from the pitch so the public long-form pages share one divider.
 */

import { GOLD } from "../design/theme";

const dim = (a: number) => `rgba(233,237,255,${a})`;

export default function Divider({ tone = GOLD }: { tone?: string }) {
  return (
    <div aria-hidden className="mx-auto flex max-w-6xl items-center gap-4 px-6">
      <span
        className="h-px flex-1"
        style={{ background: `linear-gradient(to right, transparent, ${dim(0.13)})` }}
      />
      <span className="flex items-center gap-1.5">
        <span className="h-1 w-1 rounded-full" style={{ background: dim(0.25) }} />
        <span
          className="h-1.5 w-1.5 rounded-full"
          style={{ background: tone, boxShadow: `0 0 10px ${tone}` }}
        />
        <span className="h-1 w-1 rounded-full" style={{ background: dim(0.25) }} />
      </span>
      <span
        className="h-px flex-1"
        style={{ background: `linear-gradient(to left, transparent, ${dim(0.13)})` }}
      />
    </div>
  );
}
