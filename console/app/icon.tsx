/*
 * The favicon. There was none — and no console/public/ directory at all, so the
 * browser was requesting /favicon.ico and getting the 404 page.
 *
 * Generated rather than committed as a binary: gamma (γ) on the theme's ink is
 * already the brand mark, rendered next to the wordmark in the console header
 * ("γ · binding console", src/home/Home.tsx), and generating it keeps the one
 * definition of the gold in src/design/theme.ts instead of baking it into a
 * .ico nobody can grep.
 */

import { ImageResponse } from "next/og";

export const size = { width: 32, height: 32 };
export const contentType = "image/png";

export default function Icon() {
  return new ImageResponse(
    (
      <div
        style={{
          width: "100%",
          height: "100%",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          background: "#08070c",
          color: "hsl(46,90%,68%)",
          // Slightly under the box so the glyph's descender is not clipped.
          fontSize: 26,
          lineHeight: 1,
          fontWeight: 700,
        }}
      >
        γ
      </div>
    ),
    size,
  );
}
