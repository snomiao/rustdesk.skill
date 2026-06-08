// OCR backend + text matching.
//
// Two python backends live in ../ocr/:
//   ocr-mac.py  — macOS Vision (default on darwin). Returns NORMALIZED [0,1] boxes.
//   ocr.py      — PaddleOCR (fallback / non-mac). Returns PIXEL boxes.
//
// Backend precedence: --ocr flag > RDS_OCR env > platform default (vision on mac).
//
// We normalize every backend to a uniform Box in SCREENSHOT px:
//   { text, conf, x, y, w, h }  (x,y = top-left; all in screenshot pixels)
// This is the same canonical space click-xy accepts, so a matched box center is
// directly clickable via clickScreenshotPx().

import { spawnSync } from "node:child_process";
import { existsSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";

export interface Box {
  text: string;
  conf: number;
  x: number;
  y: number;
  w: number;
  h: number;
}

export type OcrBackend = "vision" | "paddle";

const OCR_DIR = join(fileURLToPath(new URL(".", import.meta.url)), "..", "ocr");

/** Resolve which backend to use: flag > env > platform default. Pure. */
export function resolveBackend(flag?: string): OcrBackend {
  const choice = (flag || process.env.RDS_OCR || "").toLowerCase();
  if (choice === "paddle" || choice === "vision") return choice;
  return process.platform === "darwin" ? "vision" : "paddle";
}

/** Map our --lang (e.g. "ja,en") to each backend's language argument. */
export function langArg(backend: OcrBackend, lang: string): string {
  if (backend === "vision") {
    // Vision wants BCP-47-ish codes.
    const map: Record<string, string> = { ja: "ja", en: "en-US", zh: "zh-Hans", ch: "zh-Hans" };
    const codes = lang
      .split(",")
      .map((l) => map[l.trim()] || l.trim())
      .filter(Boolean);
    return codes.join(",") || "ja,en-US";
  }
  // Paddle takes a single lang; pick the first, mapping en→en, ja→japan.
  const first = (lang.split(",")[0] || "ch").trim();
  const map: Record<string, string> = { en: "en", ja: "japan", zh: "ch", ch: "ch" };
  return map[first] || first;
}

interface RawBox {
  text: string;
  confidence?: number;
  // paddle pixel coords:
  x1?: number;
  y1?: number;
  x2?: number;
  y2?: number;
  // vision normalized coords:
  x1_norm?: number;
  y1_norm?: number;
  x2_norm?: number;
  y2_norm?: number;
}

/**
 * Convert a backend's raw boxes into screenshot-px Boxes.
 * Vision boxes are normalized → multiply by png dimensions. Pure & unit-tested.
 */
export function normalizeBoxes(raw: RawBox[], pngW: number, pngH: number): Box[] {
  const out: Box[] = [];
  for (const r of raw) {
    let x1: number, y1: number, x2: number, y2: number;
    if (r.x1_norm != null) {
      x1 = r.x1_norm * pngW;
      y1 = (r.y1_norm ?? 0) * pngH;
      x2 = (r.x2_norm ?? 0) * pngW;
      y2 = (r.y2_norm ?? 0) * pngH;
    } else {
      x1 = r.x1 ?? 0;
      y1 = r.y1 ?? 0;
      x2 = r.x2 ?? 0;
      y2 = r.y2 ?? 0;
    }
    out.push({
      text: r.text,
      conf: typeof r.confidence === "number" ? r.confidence : 1,
      x: Math.round(x1),
      y: Math.round(y1),
      w: Math.round(x2 - x1),
      h: Math.round(y2 - y1),
    });
  }
  return out;
}

/** Run the OCR python backend on a PNG file and return normalized boxes. */
export function runOcr(
  pngPath: string,
  opts: { backend?: OcrBackend; lang?: string; pngW: number; pngH: number },
): Box[] {
  const backend = opts.backend || resolveBackend();
  const script = join(OCR_DIR, backend === "vision" ? "ocr-mac.py" : "ocr.py");
  if (!existsSync(script)) throw new Error(`OCR script missing: ${script}`);
  const lang = langArg(backend, opts.lang || "ja,en");
  const res = spawnSync("python3", [script, pngPath, lang], { encoding: "utf8" });
  if (res.status !== 0) {
    throw new Error(
      `OCR (${backend}) failed: ${res.stderr || res.error?.message || "unknown error"}`,
    );
  }
  let raw: RawBox[];
  try {
    raw = JSON.parse(res.stdout || "[]");
  } catch {
    throw new Error(`OCR (${backend}) produced non-JSON output: ${res.stdout.slice(0, 200)}`);
  }
  return normalizeBoxes(raw, opts.pngW, opts.pngH);
}

/** Write a PNG buffer to a temp file and OCR it. */
export function ocrBuffer(
  buf: Buffer,
  pngW: number,
  pngH: number,
  opts: { backend?: OcrBackend; lang?: string } = {},
): Box[] {
  const p = join(tmpdir(), `rds-ocr-${process.pid}.png`);
  writeFileSync(p, buf);
  return runOcr(p, { ...opts, pngW, pngH });
}

export interface MatchOptions {
  contains?: boolean; // substring match instead of exact
  nth?: number; // 1-based index to disambiguate
}

export interface MatchResult {
  box?: Box;
  matches: Box[];
  ambiguous: boolean;
}

/**
 * Find the OCR box matching `query`. Exact (case-insensitive, trimmed) by default;
 * `contains` for substring. `nth` (1-based) disambiguates; otherwise on multiple
 * matches we pick the first but flag `ambiguous` so the caller can warn. Pure.
 */
export function matchText(boxes: Box[], query: string, opts: MatchOptions = {}): MatchResult {
  const q = query.trim().toLowerCase();
  const matches = boxes.filter((b) => {
    const t = b.text.trim().toLowerCase();
    return opts.contains ? t.includes(q) : t === q;
  });
  if (matches.length === 0) return { matches, ambiguous: false };
  if (opts.nth != null) {
    return { box: matches[opts.nth - 1], matches, ambiguous: false };
  }
  return { box: matches[0], matches, ambiguous: matches.length > 1 };
}

/** Center point (screenshot px) of a box. */
export function boxCenter(b: Box): { x: number; y: number } {
  return { x: Math.round(b.x + b.w / 2), y: Math.round(b.y + b.h / 2) };
}
