#!/usr/bin/env bun
// Writes rs/src/unicode.rs: what bun says about every code point where the
// binary imitates `toLowerCase`, recorded once, so that the binary does not
// take it from Rust's own tables. Those follow the toolchain that builds the
// binary: a Rust with a newer Unicode lower-cases characters that bun keeps
// (Rust 1.100, Unicode 18: U+AB6C, U+AB6D and 16 capitals from U+1DF40 to
// U+1DF7E, which bun 1.4 leaves as they are), so the binary would answer
// according to whichever toolchain built it.
//
//   bun rs/unicode.gen.mjs            rewrite rs/src/unicode.rs
//   bun rs/unicode.gen.mjs --check    fail if this bun says otherwise
//
// For every code point it records String.fromCodePoint(c).toLowerCase() where
// that is not c itself, and whether bun's RegExp counts c as unassigned
// (\p{Cn}). Only bun gives these: node has another ICU. rs/src/js.rs answers
// from this table, and hands back what it is not certain of (see
// lowercase_unsure there). rs/parity/proof.sh runs --check, and
// rs/src/compare.rs compares the binary's answers with bun's, so a bun that
// lower-cases another way, or knows another Unicode, stops the proof.
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const TARGET = path.join(HERE, "src", "unicode.rs");
const check = process.argv.includes("--check");

if (!process.versions.bun) {
  console.error("rs/unicode.gen.mjs records bun's toLowerCase: run it with bun, not node");
  process.exit(2);
}
const unicode = process.versions.unicode;
if (!unicode) {
  console.error("this bun does not say which Unicode it has (process.versions.unicode)");
  process.exit(2);
}

// A Rust string literal that is ASCII: anything else as \u{…}.
const literal = (text) =>
  `"${[...text].map((ch) => {
    const code = ch.codePointAt(0);
    return code >= 0x20 && code < 0x7f && ch !== '"' && ch !== "\\" ? ch : `\\u{${code.toString(16)}}`;
  }).join("")}"`;
const hex = (code) => `0x${code.toString(16)}`;

const lower = [];
const assigned = [];
const unassigned = /^\p{Cn}$/u;
let from = null;
for (let code = 0; code <= 0x10ffff; code++) {
  const ch = String.fromCodePoint(code);
  const surrogate = code >= 0xd800 && code <= 0xdfff;
  if (!surrogate && ch.toLowerCase() !== ch) lower.push([code, ch.toLowerCase()]);
  // A surrogate is assigned (Cs), and no Rust char.
  const known = surrogate || !unassigned.test(ch);
  if (known && from === null) from = code;
  if (!known && from !== null) {
    assigned.push([from, code - 1]);
    from = null;
  }
}
if (from !== null) assigned.push([from, 0x10ffff]);

// What the table claims has to hold together: every character that has a
// lower case, and every character in one, is assigned.
const isAssigned = (code) => assigned.some(([a, b]) => code >= a && code <= b);
for (const [code, to] of lower) {
  for (const c of [code, ...[...to].map((ch) => ch.codePointAt(0))]) {
    if (!isAssigned(c)) {
      console.error(`U+${c.toString(16).toUpperCase()} is in bun's lower case of U+${code.toString(16).toUpperCase()}, and its RegExp says it is unassigned: this bun's ICU and its RegExp have different Unicodes`);
      process.exit(1);
    }
  }
}

const rows = (items, perLine) => {
  const lines = [];
  for (let i = 0; i < items.length; i += perLine) lines.push(`    ${items.slice(i, i + perLine).join(" ")}`);
  return lines.join("\n");
};
const text = `//! Written by rs/unicode.gen.mjs from bun (Unicode ${unicode}); do not edit.
//! \`bun rs/unicode.gen.mjs\` writes it again, \`--check\` fails when the bun at
//! hand says otherwise. rs/src/js.rs answers \`toLowerCase\` from it.

/// The Unicode of bun's ICU (\`process.versions.unicode\`), for the tests.
#[cfg(test)]
pub const VERSION: &str = ${literal(unicode)};

/// \`String.fromCodePoint(c).toLowerCase()\` for every code point whose lower
/// case is not itself, in order of code point.
#[rustfmt::skip]
pub static LOWER: [(u32, &str); ${lower.length}] = [
${rows(lower.map(([code, to]) => `(${hex(code)}, ${literal(to)}),`), 6)}
];

/// The code points bun's RegExp does not count as unassigned (\`\\P{Cn}\`), as
/// ranges in order: every character of this Unicode, private use, the
/// surrogates and nothing else.
#[rustfmt::skip]
pub static ASSIGNED: [(u32, u32); ${assigned.length}] = [
${rows(assigned.map(([a, b]) => `(${hex(a)}, ${hex(b)}),`), 6)}
];
`;

if (check) {
  const current = fs.existsSync(TARGET) ? fs.readFileSync(TARGET, "utf8") : "";
  if (current !== text) {
    const crlf = current.replace(/\r\n/g, "\n") === text;
    console.error(
      crlf
        ? "rs/src/unicode.rs differs from what rs/unicode.gen.mjs writes only in CR LF line ends: check out rs/ with LF"
        : `rs/src/unicode.rs is not what bun ${process.versions.bun} (Unicode ${unicode}) says: its toLowerCase or its Unicode has moved. ` +
            "Write the table again with `bun rs/unicode.gen.mjs`, and add the blocks where it moved to lowercase_unsure in rs/src/js.rs, " +
            "so that the binary does not answer for a bun from before the move either",
    );
    process.exit(1);
  }
  console.error(`rs/src/unicode.rs is what bun ${process.versions.bun} says (Unicode ${unicode}, ${lower.length} lower cases, ${assigned.length} ranges assigned)`);
} else {
  fs.writeFileSync(TARGET, text);
  console.error(`wrote rs/src/unicode.rs from bun ${process.versions.bun} (Unicode ${unicode}, ${lower.length} lower cases, ${assigned.length} ranges assigned)`);
}
