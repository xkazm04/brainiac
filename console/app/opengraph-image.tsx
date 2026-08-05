/*
 * The share card: the wave field, drawn statically.
 *
 * The landing page's hero is a <canvas> animating an interference pattern from
 * three team "emitters" (src/home/Home.tsx). A crawler never runs that, so this
 * route recomputes the SAME field maths at t=0 and rasterises it — the card a
 * link preview shows is the art the page actually renders, not a substitute.
 *
 * Why an inline SVG data URI rather than thousands of divs: this runs through
 * satori, which lays out every element it is given. A 1200x630 grid at the
 * hero's own spacing is ~2,600 nodes and satori is not the right tool for that.
 * So the field is ONE <img>: a tiled <pattern> for the quiet cells (a single
 * node covering the whole card) plus explicit <rect>s only where the waves
 * actually interfere — a few hundred, and the only part that carries meaning.
 *
 * No custom font is loaded on purpose. Space Grotesk would have to be fetched
 * at request time (satori needs the font BYTES; a CSS family name means nothing
 * to it), which turns a share card into a build- or runtime network dependency
 * that fails silently into a blank image. The identity here is carried by the
 * field and the gamma gold, which is the load-bearing half of the brand anyway.
 */

import { ImageResponse } from "next/og";

export const size = { width: 1200, height: 630 };
export const contentType = "image/png";
export const alt =
  "Brainiac — shared memory for coding agents, governed like code. An interference field of three team wave sources.";

// The hero's constants (Home.tsx), scaled for a 1200x630 card.
const WAVELEN = 110;
const SPACING = 16;
const BG = "#08070c";

/**
 * The three emitters, in card-relative coordinates. "payments" is out of phase
 * — the same contradiction the live hero opens on.
 *
 * Pushed to the right half, unlike the hero's centred trio: the copy occupies
 * the left ~58% of a share card and the wash that keeps it legible would
 * otherwise bury the interference pattern exactly where it is densest, leaving
 * a card that is a paragraph on black.
 */
const SOURCES = [
  { x: 0.82, y: 0.14, phase: Math.PI },
  { x: 0.93, y: 0.52, phase: 0 },
  { x: 0.72, y: 0.88, phase: 0 },
];

function fieldSvg(): string {
  const { width: W, height: H } = size;
  const k = (Math.PI * 2) / WAVELEN;
  const parts: string[] = [];

  for (let gy = SPACING / 2; gy < H; gy += SPACING) {
    for (let gx = SPACING / 2; gx < W; gx += SPACING) {
      let sum = 0;
      for (const s of SOURCES) {
        const dx = gx - s.x * W;
        const dy = gy - s.y * H;
        const d = Math.sqrt(dx * dx + dy * dy);
        // A gentler falloff than the hero's (/420): the card is a single frame,
        // so the far field has no motion to keep it alive and would otherwise
        // fade to an empty corner.
        sum += Math.sin(k * d + s.phase) / (1 + d / 700);
      }
      const b = sum / SOURCES.length;
      if (b > 0.1) {
        // constructive — gamma gold, the canonical colour
        const a = Math.min(1, (b - 0.1) * 2.1);
        const r = 1.8 + a * 3;
        parts.push(
          `<rect x="${(gx - r / 2).toFixed(1)}" y="${(gy - r / 2).toFixed(1)}" width="${r.toFixed(1)}" height="${r.toFixed(1)}" fill="hsl(46,90%,${(62 + a * 16).toFixed(0)}%)" opacity="${(0.2 + a * 0.78).toFixed(2)}"/>`,
        );
      } else if (b < -0.34) {
        // destructive — the contradiction seam
        const a = Math.min(1, (-b - 0.34) * 2.2);
        parts.push(
          `<rect x="${(gx - 1).toFixed(1)}" y="${(gy - 1).toFixed(1)}" width="2" height="2" fill="#ff5da2" opacity="${(0.08 + a * 0.36).toFixed(2)}"/>`,
        );
      }
      // quiet cells are the tiled <pattern> below — one node, not 2,000.
    }
  }

  // Emitter rings, so the card shows WHERE the sources are.
  for (const s of SOURCES) {
    const stroke = s.phase === 0 ? "hsl(46,90%,68%)" : "#ff5da2";
    parts.push(
      `<circle cx="${(s.x * W).toFixed(0)}" cy="${(s.y * H).toFixed(0)}" r="9" fill="none" stroke="${stroke}" stroke-width="1.6" opacity="0.9"/>`,
      `<circle cx="${(s.x * W).toFixed(0)}" cy="${(s.y * H).toFixed(0)}" r="26" fill="none" stroke="${stroke}" stroke-width="1.2" opacity="0.22"/>`,
    );
  }

  return `<svg xmlns="http://www.w3.org/2000/svg" width="${W}" height="${H}" viewBox="0 0 ${W} ${H}">
<defs><pattern id="q" width="${SPACING}" height="${SPACING}" patternUnits="userSpaceOnUse">
<rect x="${SPACING / 2 - 0.5}" y="${SPACING / 2 - 0.5}" width="1.4" height="1.4" fill="rgb(120,130,190)" opacity="0.3"/>
</pattern></defs>
<rect width="${W}" height="${H}" fill="${BG}"/>
<rect width="${W}" height="${H}" fill="url(#q)"/>
${parts.join("")}
</svg>`;
}

export default function OpengraphImage() {
  const src = `data:image/svg+xml;base64,${Buffer.from(fieldSvg()).toString("base64")}`;

  return new ImageResponse(
    (
      <div
        style={{
          width: "100%",
          height: "100%",
          display: "flex",
          position: "relative",
          background: BG,
        }}
      >
        {/* eslint-disable-next-line @next/next/no-img-element */}
        <img src={src} width={size.width} height={size.height} alt="" />

        {/* A wash under the copy so the field never fights the words. */}
        <div
          style={{
            position: "absolute",
            inset: 0,
            display: "flex",
            flexDirection: "column",
            justifyContent: "flex-end",
            padding: "64px 72px",
            // Opaque under the copy, then off entirely by ~72% — the field has
            // to survive on the right, which is the whole reason the emitters
            // were moved there.
            background:
              "linear-gradient(90deg, rgba(8,7,12,0.98) 0%, rgba(8,7,12,0.96) 54%, rgba(8,7,12,0.6) 68%, rgba(8,7,12,0) 82%)",
          }}
        >
          <div
            style={{
              fontSize: 22,
              letterSpacing: 6,
              textTransform: "uppercase",
              color: "hsl(46,90%,68%)",
            }}
          >
            brainiac
          </div>
          <div
            style={{
              marginTop: 22,
              fontSize: 56,
              lineHeight: 1.08,
              fontWeight: 700,
              color: "#ffffff",
              maxWidth: 660,
            }}
          >
            Shared memory for your coding agents — governed like code.
          </div>
          <div
            style={{
              marginTop: 26,
              fontSize: 23,
              lineHeight: 1.4,
              color: "rgba(233,237,255,0.62)",
              maxWidth: 640,
            }}
          >
            Permission-aware retrieval the database enforces · a human review gate before
            anything is canonical · contradictions adjudicated, not averaged.
          </div>
        </div>
      </div>
    ),
    size,
  );
}
