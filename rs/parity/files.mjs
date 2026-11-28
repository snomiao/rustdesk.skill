#!/usr/bin/env node
// Prints a directory as a parity case compares it: its mode, then each file
// in it by name, with its mode and its content. node's fs, so that it prints
// the same on Linux, macOS and Windows (where GNU stat is not there or says
// something else).
//
//   node rs/parity/files.mjs <dir>

import fs from "node:fs";
import path from "node:path";

const dir = process.argv[2];
const mode = (p) => (fs.statSync(p).mode & 0o777).toString(8);
try {
  console.log(`${path.basename(dir)}/ ${mode(dir)}`);
} catch (error) {
  console.log(`${path.basename(dir)}/ ${error.code}`);
  process.exit(0);
}
for (const name of fs.readdirSync(dir).sort()) {
  const file = path.join(dir, name);
  const stat = fs.statSync(file);
  if (stat.isDirectory()) {
    console.log(`  ${name}/ ${mode(file)}`);
    continue;
  }
  console.log(`  ${name} ${mode(file)}`);
  for (const line of fs.readFileSync(file, "utf8").split("\n")) console.log(`    | ${line}`);
}
