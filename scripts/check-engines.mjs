import { execFileSync } from "node:child_process";

const [major] = process.versions.node.split(".").map(Number);
if (major < 26) {
  console.error(`Rehow requires Node >= 26 (found ${process.version}).`);
  process.exit(1);
}
const pnpm = execFileSync("pnpm", ["--version"], { encoding: "utf8" }).trim();
if (!/^11\./.test(pnpm)) {
  console.error(`Rehow requires pnpm 11.x (found ${pnpm}).`);
  process.exit(1);
}
