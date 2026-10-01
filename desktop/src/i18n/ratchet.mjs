/**
 * The ratchet's baseline: `node src/i18n/ratchet.mjs --write` records how
 * many bare sentences each desktop source still carries
 * (`desktop/src/i18n/ratchet.baseline.json`); `--list` prints each one as
 * `path:line: literal`; with neither, the report against the committed
 * baseline. `just i18n-baseline` runs the write;
 * `scenarios/i18n.test.mjs` holds the sources to the file exactly.
 */
import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { baseline, compare, sentences } from "./ratchetModel.mjs";

const src = fileURLToPath(new URL("..", import.meta.url));
const file = fileURLToPath(new URL("./ratchet.baseline.json", import.meta.url));
const now = baseline(src);
const total = Object.values(now).reduce((a, b) => a + b, 0);

if (process.argv.includes("--list")) {
  for (const { file: at, line, text } of sentences(src)) process.stdout.write(`${at}:${line}: ${text}\n`);
  process.stdout.write(`${Object.keys(now).length} files, ${total} sentences\n`);
} else if (process.argv.includes("--write")) {
  writeFileSync(file, `${JSON.stringify(now, null, 2)}\n`);
  process.stdout.write(`${Object.keys(now).length} files with bare sentences, ${total} sentences — written to ${file}\n`);
} else {
  let was = {};
  try {
    was = JSON.parse(readFileSync(file, "utf8"));
  } catch {
    process.stdout.write("no baseline yet — run with --write\n");
  }
  const { up, down } = compare(was, now);
  for (const line of up) process.stdout.write(`up    ${line}\n`);
  for (const line of down) process.stdout.write(`down  ${line}\n`);
  process.stdout.write(`${Object.keys(now).length} files, ${total} sentences now; ${up.length} rose, ${down.length} fell\n`);
}
