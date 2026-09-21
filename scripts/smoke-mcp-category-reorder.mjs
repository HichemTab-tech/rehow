import { spawn } from "node:child_process";
import {
  chmod,
  copyFile,
  mkdir,
  mkdtemp,
  readFile,
  rm,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createInterface } from "node:readline";

const buildRoot = new URL("../target/release/", import.meta.url).pathname;
const base = await mkdtemp(join(tmpdir(), "rehow-mcp-smoke-"));
const packageRoot = join(base, "package");
const serverPath = join(packageRoot, "rehow-mcp");
const helperPath = join(packageRoot, "rehow-mcp-approve");
const workspace = join(base, "workspace");
const state = join(base, "state");
const categoryRoot = join(workspace, "categories");
const general = join(categoryRoot, "general");
const destination = join(categoryRoot, "destination");

await mkdir(packageRoot, { mode: 0o700 });
await copyFile(join(buildRoot, "rehow-mcp"), serverPath);
await copyFile(join(buildRoot, "rehow-mcp-approve"), helperPath);
await chmod(serverPath, 0o755);
await chmod(helperPath, 0o755);
await mkdir(general, { recursive: true, mode: 0o700 });
await mkdir(destination, { mode: 0o700 });
await mkdir(state, { mode: 0o700 });
await writeFile(
  join(workspace, "rehow.yaml"),
  'schema_version: 1\nworkspace_id: 0199a1d7-7b0d-7d8e-a38d-f936c5735e10\nname: "Smoke Workspace"\ncreated_at: "2026-09-20T00:00:00Z"\nupdated_at: "2026-09-20T00:00:00Z"\n',
  { mode: 0o600 },
);
await writeFile(
  join(general, "category.yaml"),
  'schema_version: 1\nid: 0199a1d7-7b0d-7d8e-a38d-f936c5735e11\nname: General\norder: 1000\nstatus: active\ncreated_at: "2026-09-20T00:00:00Z"\nupdated_at: "2026-09-20T00:00:00Z"\n',
  { mode: 0o600 },
);
await writeFile(
  join(destination, "category.yaml"),
  'schema_version: 1\nid: 0199a1d7-7b0d-7d8e-a38d-f936c5735e13\nname: Destination\norder: 2000\nstatus: active\ncreated_at: "2026-09-20T00:00:00Z"\nupdated_at: "2026-09-20T00:00:00Z"\n',
  { mode: 0o600 },
);

const child = spawn(
  serverPath,
  [
    "--workspace",
    `smoke=${workspace}`,
    "--allow-write",
    "smoke",
    "--allow-broad",
    "smoke",
  ],
  {
    env: { ...process.env, XDG_STATE_HOME: state },
    stdio: ["pipe", "pipe", "pipe"],
  },
);
child.stderr.setEncoding("utf8");
child.stderr.on("data", (chunk) => process.stderr.write(chunk));
const lines = createInterface({ input: child.stdout });
const pending = new Map();
lines.on("line", (line) => {
  const message = JSON.parse(line);
  const waiter = pending.get(message.id);
  if (waiter) {
    pending.delete(message.id);
    waiter(message);
  }
});

function request(id, method, params) {
  return new Promise((resolve) => {
    pending.set(id, resolve);
    child.stdin.write(
      `${JSON.stringify({ jsonrpc: "2.0", id, method, params })}\n`,
    );
  });
}

function structured(response) {
  if (response.error) throw new Error(JSON.stringify(response.error));
  return response.result.structuredContent;
}

try {
  await request(1, "initialize", {
    protocolVersion: "2025-11-25",
    capabilities: {},
    clientInfo: { name: "release-smoke", version: "1" },
  });
  const listed = structured(
    await request(2, "tools/call", { name: "workspaces_list", arguments: {} }),
  );
  const handle = listed.workspaces[0].workspaceHandle;
  const categories = structured(
    await request(3, "tools/call", {
      name: "categories_list",
      arguments: { workspaceHandle: handle },
    }),
  );
  const reversed = [...categories.categories].reverse().map((item) => item.id);
  const approvedPlan = structured(
    await request(4, "tools/call", {
      name: "category_reorder_plan",
      arguments: {
        workspaceHandle: handle,
        expectedCategorySnapshot: categories.categorySnapshot,
        orderedCategoryIds: reversed,
      },
    }),
  );
  if (!approvedPlan.planToken) throw new Error(JSON.stringify(approvedPlan));
  process.stderr.write("FIRST_DIALOG: click Approve\n");
  const approved = structured(
    await request(5, "tools/call", {
      name: "category_reorder",
      arguments: { workspaceHandle: handle, planToken: approvedPlan.planToken },
    }),
  );
  if (approved.status !== "applied" || approved.changed !== true) {
    throw new Error(`approval did not apply: ${JSON.stringify(approved)}`);
  }

  const afterApprove = structured(
    await request(6, "tools/call", {
      name: "categories_list",
      arguments: { workspaceHandle: handle },
    }),
  );
  const beforeDeny = await Promise.all([
    readFile(join(general, "category.yaml")),
    readFile(join(destination, "category.yaml")),
  ]);
  const original = [...afterApprove.categories]
    .reverse()
    .map((item) => item.id);
  const deniedPlan = structured(
    await request(7, "tools/call", {
      name: "category_reorder_plan",
      arguments: {
        workspaceHandle: handle,
        expectedCategorySnapshot: afterApprove.categorySnapshot,
        orderedCategoryIds: original,
      },
    }),
  );
  if (!deniedPlan.planToken) throw new Error(JSON.stringify(deniedPlan));
  process.stderr.write("SECOND_DIALOG: click Cancel\n");
  const denied = structured(
    await request(8, "tools/call", {
      name: "category_reorder",
      arguments: { workspaceHandle: handle, planToken: deniedPlan.planToken },
    }),
  );
  if (denied.status !== "denied" || denied.changed !== false) {
    throw new Error(`denial changed state: ${JSON.stringify(denied)}`);
  }
  const afterDeny = await Promise.all([
    readFile(join(general, "category.yaml")),
    readFile(join(destination, "category.yaml")),
  ]);
  if (!beforeDeny.every((value, index) => value.equals(afterDeny[index]))) {
    throw new Error("denied approval changed Category bytes");
  }
  process.stdout.write("SMOKE_PASS: packaged native approve and deny paths\n");
} finally {
  child.stdin.end();
  await new Promise((resolve) => child.once("exit", resolve));
  await rm(base, { recursive: true, force: true });
}
