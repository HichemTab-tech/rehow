import { readFile } from "node:fs/promises";
import { blake3 } from "@noble/hashes/blake3.js";
import { bytesToHex } from "@noble/hashes/utils.js";

if (process.argv.length !== 3) {
  process.stderr.write("usage: node scripts/blake3-file.mjs FILE\n");
  process.exit(2);
}

const bytes = await readFile(process.argv[2]);
process.stdout.write(`${bytesToHex(blake3(bytes))}\n`);
