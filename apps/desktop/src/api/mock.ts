// Development-only in-memory backend used when the UI runs in a plain browser
// (`pnpm dev`). It mimics the native command contract closely enough to design
// and test the interface without Tauri. It is never bundled in production.
import type {
  SafeMarkdownContainer,
  SafeMarkdownDocument,
  SafeMarkdownEvent,
} from "@rehow/contracts";
import type {
  GitStatusPayload,
  GuideBodyPayload,
  GuideSummary,
  SearchHitPayload,
  WorkspacePayload,
} from "./payloads";

type Category = WorkspacePayload["categories"][number];
type StoredGuide = Omit<GuideBodyPayload, "document"> & {
  status: GuideSummary["status"];
  updatedAt: string;
  attachments: GuideSummary["attachments"];
};

const delay = (ms = 60) => new Promise((resolve) => setTimeout(resolve, ms));
let counter = 1000;
const nextId = () =>
  `0199a1d7-7b0d-7d8e-a38d-${(counter++).toString(16).padStart(12, "0")}`;
const nextRevision = () => `rev:${Math.random().toString(36).slice(2, 12)}`;

const categories: Category[] = [
  cat("Linux System", "Host setup, boot, devices and desktop integration."),
  cat("Docker & Containers", "Images, compose stacks and registries."),
  cat("Networking", "DNS, VPNs, firewalls and SSH."),
  cat("Databases", null),
  cat("Old Laptop", null, "archived"),
];

function cat(
  name: string,
  summary: string | null,
  status: Category["status"] = "active",
): Category {
  return {
    id: nextId(),
    name,
    summary,
    status,
    revision: nextRevision(),
    guideCount: 0,
  };
}

const LONG_BODY = `## Steps

1. Download the release archive and its checksum from the vendor.

   \`\`\`bash
   curl -LO https://example.com/app/releases/app-2.4.1-linux-x86_64.tar.gz
   curl -LO https://example.com/app/releases/app-2.4.1-linux-x86_64.tar.gz.sha256
   \`\`\`

2. Verify the checksum before extracting anything.

   \`\`\`bash
   sha256sum --check app-2.4.1-linux-x86_64.tar.gz.sha256
   \`\`\`

3. Extract under \`~/Programs\` so it stays out of system paths.

   \`\`\`bash
   mkdir -p ~/Programs && tar -xzf app-2.4.1-linux-x86_64.tar.gz -C ~/Programs
   \`\`\`

4. Link the binary into your \`PATH\`.

   \`\`\`bash
   ln -sf ~/Programs/app-2.4.1/bin/app ~/.local/bin/app
   \`\`\`

## Verify

Run \`app --version\` and confirm it prints **2.4.1**.

## Rollback

- Remove the symlink: \`rm ~/.local/bin/app\`
- Delete the extracted folder.

## Troubleshooting

| Symptom | Fix |
| --- | --- |
| \`command not found\` | Make sure \`~/.local/bin\` is on your \`PATH\`. |
| Checksum mismatch | Download again; never run an unverified archive. |
`;

const guides: StoredGuide[] = [
  guide({
    title: "Install a downloaded tarball",
    summary: "Install an application archive safely under your home directory.",
    category: 0,
    status: "Tested",
    tags: ["arch-linux", "installation"],
    aliases: ["manual install", "tar.gz"],
    body: LONG_BODY,
    warnings: ["Back up an existing installation before replacing it."],
    prerequisites: [
      "A trusted tarball downloaded from the vendor",
      "Enough space under ~/Programs",
    ],
    filesAffected: ["~/Programs/app-2.4.1", "~/.local/bin/app"],
    appliesTo: [
      {
        platform: "Arch Linux",
        versions: "rolling",
        notes: "Hyprland session",
      },
    ],
    references: [
      { title: "Vendor installation manual", url: "https://example.com/docs" },
    ],
    days: 2,
    attachments: [{ id: nextId(), title: "app.desktop", sizeBytes: 412 }],
  }),
  guide({
    title: "Create a desktop launcher",
    summary:
      "Add a local application to the desktop menu with a .desktop file.",
    category: 0,
    status: "Draft",
    tags: ["desktop", "xdg"],
    body: "## Steps\n\n1. Create `~/.local/share/applications/app.desktop`:\n\n   ```ini\n   [Desktop Entry]\n   Name=App\n   Exec=/home/me/.local/bin/app\n   Type=Application\n   ```\n\n2. Refresh the menu cache with `update-desktop-database ~/.local/share/applications`.\n",
    days: 3,
  }),
  guide({
    title: "Fix NVIDIA flicker on Wayland",
    summary:
      "Disable explicit sync for WebKitGTK apps that crash with protocol error 71.",
    category: 0,
    status: "Stale",
    tags: ["nvidia", "wayland", "hyprland"],
    aliases: ["Error 71 (Protocol error)"],
    body: "## Symptom\n\nThe app exits with `Gdk-Message: Error 71 (Protocol error) dispatching to Wayland display.`\n\n## Fix\n\n```bash\nexport __NV_DISABLE_EXPLICIT_SYNC=1\n```\n\nAdd it to `~/.config/hypr/hyprland.conf` as an `env` line to make it permanent.\n",
    days: 40,
  }),
  guide({
    title: "Prune unused Docker images",
    summary:
      "Reclaim disk space from dangling images, stopped containers and build cache.",
    category: 1,
    status: "Tested",
    tags: ["docker", "disk-space"],
    aliases: ["docker cleanup"],
    body: '## Steps\n\n```bash\ndocker system df\ndocker image prune -a --filter "until=168h"\ndocker builder prune\n```\n\n> Volumes are **not** removed unless you pass `--volumes`.\n',
    warnings: ["`--volumes` deletes named volumes and their data permanently."],
    days: 1,
  }),
  guide({
    title: "Compose stack with Postgres and pgAdmin",
    summary:
      "A local database stack with persistent volumes and a health check.",
    category: 1,
    status: "Draft",
    tags: ["docker", "postgres"],
    body: "## compose.yaml\n\n```yaml\nservices:\n  db:\n    image: postgres:17\n    environment:\n      POSTGRES_PASSWORD: example\n    volumes:\n      - pgdata:/var/lib/postgresql/data\nvolumes:\n  pgdata:\n```\n",
    days: 6,
  }),
  guide({
    title: "Set up WireGuard client",
    summary: "Connect to the home VPN with wg-quick and a systemd unit.",
    category: 2,
    status: "Tested",
    tags: ["vpn", "wireguard", "systemd"],
    body: "## Steps\n\n1. Install: `sudo pacman -S wireguard-tools`\n2. Copy the config to `/etc/wireguard/wg0.conf`.\n3. Enable it:\n\n   ```bash\n   sudo systemctl enable --now wg-quick@wg0\n   ```\n",
    days: 12,
  }),
  guide({
    title: "Generate an SSH key for GitHub",
    summary:
      "Create an ed25519 key, add it to the agent and register it on GitHub.",
    category: 2,
    status: "Tested",
    tags: ["ssh", "git", "github"],
    aliases: ["ssh-keygen"],
    body: '## Steps\n\n```bash\nssh-keygen -t ed25519 -C "you@example.com"\neval "$(ssh-agent -s)"\nssh-add ~/.ssh/id_ed25519\n```\n\nThen paste `~/.ssh/id_ed25519.pub` into **GitHub → Settings → SSH keys**.\n',
    days: 20,
  }),
  guide({
    title: "Restore a Postgres dump",
    summary: "Restore a custom-format pg_dump into a fresh database.",
    category: 3,
    status: "Draft",
    tags: ["postgres", "backup"],
    body: "## Steps\n\n```bash\ncreatedb myapp_restore\npg_restore --no-owner -d myapp_restore backup.dump\n```\n",
    days: 9,
  }),
  guide({
    title: "Reset the old laptop BIOS password",
    summary: "Clear the CMOS on the ThinkPad T420.",
    category: 4,
    status: "Archived",
    tags: ["hardware"],
    body: "Remove the CMOS battery for 60 seconds.\n",
    days: 200,
  }),
];

function guide(input: {
  title: string;
  summary: string;
  category: number;
  status: GuideSummary["status"];
  tags?: string[];
  aliases?: string[];
  body: string;
  warnings?: string[];
  prerequisites?: string[];
  filesAffected?: string[];
  appliesTo?: GuideBodyPayload["appliesTo"];
  references?: GuideBodyPayload["references"];
  attachments?: GuideSummary["attachments"];
  days: number;
  origin?: "managed" | "unadopted";
}): StoredGuide {
  const updated = new Date(Date.now() - input.days * 86_400_000).toISOString();
  return {
    id: nextId(),
    origin: input.origin ?? "managed",
    title: input.title,
    summary: input.summary,
    categoryId: categories[input.category]?.id ?? "",
    tags: input.tags ?? [],
    aliases: input.aliases ?? [],
    revision: nextRevision(),
    body: input.body,
    createdAt: new Date(
      Date.now() - (input.days + 30) * 86_400_000,
    ).toISOString(),
    lastVerifiedAt: input.status === "Tested" ? updated : null,
    appliesTo: input.appliesTo ?? [],
    prerequisites: input.prerequisites ?? [],
    warnings: input.warnings ?? [],
    filesAffected: input.filesAffected ?? [],
    relatedGuideIds: [],
    references: input.references ?? [],
    status: input.status,
    updatedAt: updated,
    attachments: input.attachments ?? [],
  };
}

guides.push(
  guide({
    title: "Rotate nginx logs",
    summary: "",
    category: 2,
    status: "Draft",
    origin: "unadopted",
    body: "# Rotate nginx logs\n\nQuick notes I wrote by hand.\n\n```bash\nsudo logrotate -f /etc/logrotate.d/nginx\n```\n",
    days: 4,
  }),
);

// `?many=5000` adds synthetic Guides to exercise large-list performance.
const many = Number(
  new URLSearchParams(globalThis.location?.search ?? "").get("many") ?? 0,
);
for (let index = 0; index < Math.min(many, 20_000); index++) {
  guides.push(
    guide({
      title: `Synthetic guide ${index + 1}`,
      summary: `Generated entry number ${index + 1} for scrolling tests.`,
      category: index % 4,
      status: index % 7 === 0 ? "Tested" : "Draft",
      tags: [`tag-${index % 25}`],
      body: `## Steps\n\nRun \`echo ${index}\`.\n`,
      days: (index % 300) + 1,
    }),
  );
}

if (guides[0] && guides[1]) guides[0].relatedGuideIds = [guides[1].id];

let opened = false;
let watchSession = 1;
const gitStatus: GitStatusPayload = {
  kind: "ready",
  version: "2.55.0",
  branch: "main",
  detached: false,
  upstream: "origin/main",
  divergenceKnown: true,
  ahead: 1,
  behind: 0,
  changed: 3,
  untracked: 1,
  conflicts: 0,
  remotes: ["origin"],
  remoteCount: 1,
  identity: "configured",
  globalIdentity: "available",
  failure: null,
  sandbox: "bubblewrap",
};

function summary(stored: StoredGuide): GuideSummary {
  const category = categories.find((item) => item.id === stored.categoryId);
  return {
    id: stored.id,
    origin: stored.origin,
    title: stored.title,
    summary: stored.summary,
    categoryId: stored.categoryId,
    category: category?.name ?? "Unknown",
    status: stored.status,
    updatedAt: stored.updatedAt,
    tags: stored.tags,
    aliases: stored.aliases,
    attachments: stored.attachments,
    matches: `${stored.aliases.join(" ")} ${stored.tags.join(" ")} ${stored.body}`,
  };
}

function workspace(): WorkspacePayload {
  for (const category of categories) {
    category.guideCount = guides.filter(
      (item) => item.categoryId === category.id,
    ).length;
  }
  return {
    id: "0199a19f-4d15-7ca7-a645-e75311caa710",
    name: "Personal Guides",
    categories: categories.map((category) => ({ ...category })),
    guides: [...guides]
      .sort((a, b) => b.updatedAt.localeCompare(a.updatedAt))
      .map(summary),
    issueCount: 1,
    issues: [
      {
        kind: "malformedGuide",
        relativePath: "categories/networking--a1b2c3/notes.md",
        diagnostic: "yamlSyntax",
        field: null,
        line: 4,
        column: 9,
      },
    ],
    issuesTruncated: false,
    facts: {
      writable: true,
      existingGitRepository: true,
      nestedGitRepository: false,
      gitProbeComplete: true,
      unsafeGitMarker: false,
    },
    watchStatus: "active",
    watchSession: String(watchSession),
  };
}

function body(stored: StoredGuide): GuideBodyPayload {
  const { status: _s, updatedAt: _u, attachments: _a, ...rest } = stored;
  return { ...rest, document: renderMarkdown(stored.body) };
}

function find(guideId: string) {
  const stored = guides.find((item) => item.id === guideId);
  if (!stored) throw { code: "guideNotFound", message: "Guide not found." };
  return stored;
}

function touch(stored: StoredGuide) {
  stored.revision = nextRevision();
  stored.updatedAt = new Date().toISOString();
  if (stored.status === "Tested") stored.status = "Stale";
}

function search(query: string, scope: string): SearchHitPayload[] {
  const terms = query.toLowerCase().split(/\s+/).filter(Boolean);
  const hits: Array<SearchHitPayload & { score: number }> = [];
  for (const stored of guides) {
    if (scope === "active" && stored.status === "Archived") continue;
    if (
      !["active", "all"].includes(scope) &&
      stored.status.toLowerCase() !== scope
    )
      continue;
    const fields: Array<[SearchHitPayload["excerptField"], string, number]> = [
      ["title", stored.title, 100],
      ["alias", stored.aliases.join(" · "), 90],
      ["tag", stored.tags.join(" · "), 45],
      ["summary", stored.summary, 35],
      ["body", stored.body.replace(/\s+/g, " "), 10],
    ];
    if (
      !terms.every((term) =>
        fields.some(([, text]) => text.toLowerCase().includes(term)),
      )
    )
      continue;
    const matched = fields.filter(([, text]) =>
      terms.some((term) => text.toLowerCase().includes(term)),
    );
    const best = matched[0];
    if (!best) continue;
    const [field, text, score] = best;
    const lower = text.toLowerCase();
    const first = terms[0] ?? "";
    const at = Math.max(0, lower.indexOf(first) - 40);
    const excerpt = text.slice(at, at + 160);
    const highlights: Array<{ start: number; end: number }> = [];
    const excerptLower = excerpt.toLowerCase();
    let cursor = 0;
    while (first && cursor < excerptLower.length && highlights.length < 8) {
      const found = excerptLower.indexOf(first, cursor);
      if (found < 0) break;
      highlights.push({ start: found, end: found + first.length });
      cursor = found + first.length;
    }
    hits.push({
      guideId: stored.id,
      excerpt,
      excerptField: field,
      matchedFields: matched.map(([name]) => name),
      highlights,
      score,
    });
  }
  return hits
    .sort((a, b) => b.score - a.score)
    .map(({ score: _score, ...hit }) => hit);
}

type Handler = (args: Record<string, unknown>) => unknown;

const handlers: Record<string, Handler> = {
  load_desktop_preferences: () => ({
    syncOnStart: false,
    syncIntervalMinutes: null,
  }),
  save_desktop_preferences: (args) => ({
    syncOnStart: args["syncOnStart"],
    syncIntervalMinutes: args["syncIntervalMinutes"],
  }),
  restore_recent_workspace: () => {
    if (new URLSearchParams(location.search).has("welcome"))
      return { kind: "none" };
    opened = true;
    return { kind: "restored", workspace: workspace() };
  },
  remember_open_workspace: () => null,
  mcp_setup_open_workspace: () => ({
    workspacePath: "/home/me/Documents/Personal Guides",
    mcpBinary: "/usr/bin/rehow-mcp",
  }),
  choose_workspace: () => {
    opened = true;
    return { kind: "compatible", workspace: workspace() };
  },
  refresh_open_workspace: () => {
    if (!opened)
      throw { code: "workspaceUnavailable", message: "No Workspace is open." };
    return workspace();
  },
  read_open_guide: (args) => body(find(String(args["guideId"]))),
  preview_external_open_guide: (args) => body(find(String(args["guideId"]))),
  search_open_workspace: (args) =>
    search(String(args["query"]), String(args["scope"])),
  render_markdown_preview: (args) => renderMarkdown(String(args["body"])),
  save_open_guide_body: (args) => {
    const stored = find(String(args["guideId"]));
    if (args["expectedRevision"] !== stored.revision)
      throw {
        code: "revisionConflict",
        message: "This Guide changed on disk.",
      };
    const changed = stored.body !== args["body"];
    if (stored.origin === "unadopted") {
      stored.origin = "managed";
      stored.id = nextId();
    }
    if (changed) {
      stored.body = String(args["body"]);
      touch(stored);
    }
    return { workspace: workspace(), guide: body(stored), changed };
  },
  save_open_guide_details: (args) => {
    const stored = find(String(args["guideId"]));
    if (stored.origin === "unadopted") {
      stored.origin = "managed";
      stored.id = nextId();
    }
    stored.title = String(args["title"]);
    stored.summary = String(args["summary"]);
    stored.categoryId = String(args["categoryId"]);
    stored.tags = args["tags"] as string[];
    stored.aliases = args["aliases"] as string[];
    touch(stored);
    return { workspace: workspace(), guide: body(stored), changed: true };
  },
  set_open_guide_archived: (args) => {
    const stored = find(String(args["guideId"]));
    stored.status = args["archived"] ? "Archived" : "Draft";
    stored.revision = nextRevision();
    return { workspace: workspace(), guide: body(stored), changed: true };
  },
  approve_open_guide_tested: (args) => {
    const stored = find(String(args["guideId"]));
    stored.status = "Tested";
    stored.lastVerifiedAt = new Date().toISOString();
    stored.revision = nextRevision();
    return { workspace: workspace(), guide: body(stored), changed: true };
  },
  remove_open_guide: (args) => {
    const index = guides.findIndex((item) => item.id === args["guideId"]);
    if (index >= 0) guides.splice(index, 1);
    return { workspace: workspace(), removed: true };
  },
  create_open_guide: (args) => {
    const stored: StoredGuide = {
      ...guide({
        title: String(args["title"]),
        summary: String(args["summary"]),
        category: 0,
        status: "Draft",
        body: "## Steps\n\n1. \n",
        days: 0,
      }),
      categoryId: String(args["categoryId"]),
    };
    guides.push(stored);
    return { workspace: workspace(), guide: body(stored) };
  },
  create_open_category: (args) => {
    const created = cat(
      String(args["name"]),
      (args["summary"] as string | null) ?? null,
    );
    categories.push(created);
    return { workspace: workspace(), categoryId: created.id };
  },
  save_open_category_details: (args) => {
    const category = categories.find((item) => item.id === args["categoryId"]);
    if (category) {
      category.name = String(args["name"]);
      category.summary = (args["summary"] as string | null) ?? null;
      category.revision = nextRevision();
    }
    return { workspace: workspace(), changed: true };
  },
  set_open_category_archived: (args) => {
    const category = categories.find((item) => item.id === args["categoryId"]);
    if (category) {
      category.status = args["archived"] ? "archived" : "active";
      category.revision = nextRevision();
    }
    return { workspace: workspace(), changed: true };
  },
  reorder_open_categories: (args) => {
    const order = (args["ordered"] as Array<{ id: string }>).map(
      (item) => item.id,
    );
    categories.sort((a, b) => order.indexOf(a.id) - order.indexOf(b.id));
    return { workspace: workspace(), changed: true };
  },
  remove_open_category: (args) => {
    const index = categories.findIndex(
      (item) => item.id === args["categoryId"],
    );
    if (index >= 0 && categories[index]?.guideCount === 0)
      categories.splice(index, 1);
    else
      throw {
        code: "categoryNotEmpty",
        message: "Move or delete its Guides first.",
      };
    return { workspace: workspace(), changed: true };
  },
  import_open_guide_attachment: (args) => {
    const stored = find(String(args["guideId"]));
    stored.attachments.push({
      id: nextId(),
      title: "notes.txt",
      sizeBytes: 2048,
    });
    stored.revision = nextRevision();
    return { workspace: workspace(), guide: body(stored), changed: true };
  },
  remove_open_guide_attachment: (args) => {
    const stored = find(String(args["guideId"]));
    stored.attachments = stored.attachments.filter(
      (item) => item.id !== args["attachmentId"],
    );
    stored.revision = nextRevision();
    return { workspace: workspace(), guide: body(stored), changed: true };
  },
  preview_open_guide_attachment_text: () => ({
    title: "app.desktop",
    text: "[Desktop Entry]\nName=App\nExec=app\nType=Application\n",
    sizeBytes: 54,
    sanitized: false,
  }),
  export_open_guide_attachment: () => ({ kind: "exported", sizeBytes: 54 }),
  reveal_open_guide_attachment_parent: () => ({ revealed: true }),
  open_guide_file: () => ({ requested: true }),
  open_guide_external_link: () => ({ requested: true }),
  git_status_open_workspace: () => ({ ...gitStatus }),
  git_commit_plan_open_workspace: () => ({
    planToken: `plan:v1:${"a".repeat(43)}`,
    branch: "main",
    created: 1,
    updated: 2,
    deleted: 0,
    unrelatedChanges: 1,
  }),
  git_commit_apply_open_workspace: () => {
    gitStatus.changed = 0;
    gitStatus.untracked = 1;
    gitStatus.ahead += 1;
    return { committed: true, workspaceChanged: false };
  },
  git_commit_cancel_open_workspace: () => ({ cancelled: true }),
  git_sync_inspect_open_workspace: () => ({
    planToken: `plan:v1:${"b".repeat(43)}`,
    remoteName: "origin",
    transport: "https",
    host: "github.com",
    ancestry: gitStatus.ahead > 0 ? "localAhead" : "upToDate",
    localChanges: gitStatus.changed > 0,
    workspaceMatchesRemote: true,
  }),
  git_sync_cancel_active: () => ({ cancellationRequested: true }),
  git_push_apply_open_workspace: () => {
    gitStatus.ahead = 0;
    return { kind: "completed", pushed: true };
  },
  git_sync_apply_open_workspace: () => ({
    kind: "completed",
    workspace: workspace(),
    changed: false,
  }),
  git_remote_plan_open_workspace: (args) => ({
    planToken: `plan:v1:${"c".repeat(43)}`,
    remoteName: args["remoteName"],
    transport: "https",
    host: "github.com",
  }),
  git_remote_apply_open_workspace: () => ({ connected: true }),
  git_identity_plan_global_open_workspace: () => ({
    planToken: `plan:v1:${"d".repeat(43)}`,
    source: "global",
  }),
  git_identity_plan_manual_open_workspace: () => ({
    planToken: `plan:v1:${"d".repeat(43)}`,
    source: "manual",
  }),
  git_identity_apply_open_workspace: () => ({ configured: true }),
  git_initialize_plan_open_workspace: () => ({
    planToken: `plan:v1:${"e".repeat(43)}`,
    initialBranch: "main",
  }),
  git_initialize_apply_open_workspace: () => ({ initialized: true }),
};

export function createMockTransport() {
  return {
    invoke: async (command: string, args: Record<string, unknown> = {}) => {
      await delay();
      const handler = handlers[command];
      if (!handler)
        throw { code: "unsupported", message: `Mock has no ${command}.` };
      return structuredClone(handler(args));
    },
    listen: async (_event: string, _handler: (payload: unknown) => void) => {
      watchSession += 0;
      return () => undefined;
    },
  };
}

// --- Minimal Markdown → safe event stream, good enough for design work. ---

function renderMarkdown(source: string): SafeMarkdownDocument {
  const events: SafeMarkdownEvent[] = [];
  const open = (node: SafeMarkdownContainer) =>
    events.push({ kind: "start", node });
  const close = () => events.push({ kind: "end" });
  const lines = source.replace(/\r\n/g, "\n").split("\n");
  let index = 0;
  const listStack: number[] = [];

  function inline(text: string) {
    const pattern = /(`[^`]+`|\*\*[^*]+\*\*|\[[^\]]+\]\([^)]+\))/g;
    let last = 0;
    for (const match of text.matchAll(pattern)) {
      const at = match.index ?? 0;
      if (at > last) events.push({ kind: "text", value: text.slice(last, at) });
      const token = match[0];
      if (token.startsWith("`")) {
        events.push({ kind: "inlineCode", value: token.slice(1, -1) });
      } else if (token.startsWith("**")) {
        open({ kind: "strong" });
        events.push({ kind: "text", value: token.slice(2, -2) });
        close();
      } else {
        const [, label, url] = /\[([^\]]+)\]\(([^)]+)\)/.exec(token) ?? [];
        open({ kind: "link", destination: url ?? null, title: null });
        events.push({ kind: "text", value: label ?? "" });
        close();
      }
      last = at + token.length;
    }
    if (last < text.length)
      events.push({ kind: "text", value: text.slice(last) });
  }

  function closeLists(depth: number) {
    while (listStack.length > depth) {
      close(); // item
      close(); // list
      listStack.pop();
    }
  }

  while (index < lines.length) {
    const raw = lines[index] ?? "";
    const line = raw.replace(/^ {3}/, "");
    if (line.trim() === "") {
      index++;
      continue;
    }
    const fence = /^\s*```(\w*)/.exec(line);
    if (fence) {
      const code: string[] = [];
      index++;
      while (index < lines.length && !/^\s*```/.test(lines[index] ?? "")) {
        code.push((lines[index] ?? "").replace(/^ {3}/, ""));
        index++;
      }
      index++;
      open({ kind: "codeBlock", language: fence[1] || null });
      events.push({ kind: "text", value: `${code.join("\n")}\n` });
      close();
      continue;
    }
    const heading = /^(#{1,6})\s+(.*)$/.exec(line);
    if (heading) {
      closeLists(0);
      open({ kind: "heading", level: heading[1]?.length ?? 2 });
      inline(heading[2] ?? "");
      close();
      index++;
      continue;
    }
    if (line.startsWith("> ")) {
      closeLists(0);
      open({ kind: "blockQuote" });
      open({ kind: "paragraph" });
      inline(line.slice(2));
      close();
      close();
      index++;
      continue;
    }
    if (line.startsWith("|")) {
      closeLists(0);
      const rows: string[][] = [];
      while (index < lines.length && (lines[index] ?? "").startsWith("|")) {
        const cells = (lines[index] ?? "")
          .split("|")
          .slice(1, -1)
          .map((cell) => cell.trim());
        if (!cells.every((cell) => /^-+$/.test(cell))) rows.push(cells);
        index++;
      }
      open({ kind: "table", alignments: (rows[0] ?? []).map(() => "none") });
      rows.forEach((row, rowIndex) => {
        open(rowIndex === 0 ? { kind: "tableHead" } : { kind: "tableRow" });
        for (const cell of row) {
          open({ kind: "tableCell" });
          inline(cell);
          close();
        }
        close();
      });
      close();
      continue;
    }
    const item = /^(\d+\.|-)\s+(.*)$/.exec(line);
    if (item) {
      const ordered = item[1] !== "-";
      if (listStack.length === 0) {
        open({ kind: "list", start: ordered ? 1 : null });
        listStack.push(1);
      } else {
        close(); // previous item
      }
      open({ kind: "item" });
      inline(item[2] ?? "");
      index++;
      continue;
    }
    if (raw.startsWith("   ") && listStack.length > 0) {
      open({ kind: "paragraph" });
      inline(line.trim());
      close();
      index++;
      continue;
    }
    closeLists(0);
    open({ kind: "paragraph" });
    inline(line);
    close();
    index++;
  }
  closeLists(0);
  return { version: 1, events };
}
