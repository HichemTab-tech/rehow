import * as Tabs from "@radix-ui/react-tabs";
import {
  Bot,
  Check,
  Copy,
  GitBranch,
  Monitor,
  Moon,
  Palette,
  Sun,
} from "lucide-react";
import { useEffect, useState } from "react";
import { toast } from "sonner";
import { api } from "@/api/client";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogHeader } from "@/components/ui/dialog";
import { Select } from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { setTheme, type ThemeChoice, useTheme } from "@/lib/theme";
import { cn } from "@/lib/utils";
import { closeDialog, reportError, useApp } from "@/store/app";

const INTERVALS: Array<[string, number | null]> = [
  ["Never", null],
  ["Every 15 minutes", 15],
  ["Every 30 minutes", 30],
  ["Every hour", 60],
  ["Every 4 hours", 240],
  ["Once a day", 1440],
];

export function SettingsDialog() {
  const open = useApp((state) => state.dialog === "settings");
  return (
    <Dialog open={open} onOpenChange={(next) => !next && closeDialog()}>
      <DialogContent className="top-[8vh] h-[min(620px,84vh)] max-h-none w-[min(94vw,760px)]">
        <DialogHeader title="Settings" />
        <Tabs.Root
          defaultValue="appearance"
          orientation="vertical"
          className="flex min-h-0 flex-1 border-t border-border"
        >
          <Tabs.List
            aria-label="Settings sections"
            className="flex w-44 shrink-0 flex-col gap-px border-r border-border bg-sidebar p-2"
          >
            {(
              [
                ["appearance", "Appearance", Palette],
                ["git", "Git sync", GitBranch],
                ["ai", "AI assistants", Bot],
              ] as const
            ).map(([value, label, Icon]) => (
              <Tabs.Trigger
                key={value}
                value={value}
                className="flex h-8 items-center gap-2 rounded-md px-2 text-left text-sm text-muted hover:bg-hover hover:text-text data-[state=active]:bg-active data-[state=active]:font-medium data-[state=active]:text-text [&_svg]:size-4"
              >
                <Icon /> {label}
              </Tabs.Trigger>
            ))}
          </Tabs.List>
          <div className="min-w-0 flex-1 overflow-y-auto p-6">
            <Tabs.Content value="appearance">
              <AppearanceSettings />
            </Tabs.Content>
            <Tabs.Content value="git">
              <GitSettings />
            </Tabs.Content>
            <Tabs.Content value="ai">
              <AiSettings />
            </Tabs.Content>
          </div>
        </Tabs.Root>
      </DialogContent>
    </Dialog>
  );
}

function Section({
  title,
  description,
  children,
}: {
  title: string;
  description?: string;
  children: React.ReactNode;
}) {
  return (
    <section className="space-y-3">
      <div>
        <h3 className="text-sm font-semibold">{title}</h3>
        {description ? (
          <p className="mt-0.5 text-xs text-muted">{description}</p>
        ) : null}
      </div>
      {children}
    </section>
  );
}

function AppearanceSettings() {
  const choice = useTheme((state) => state.choice);
  const options: Array<[ThemeChoice, string, typeof Sun]> = [
    ["system", "System", Monitor],
    ["light", "Light", Sun],
    ["dark", "Dark", Moon],
  ];
  return (
    <Section
      title="Theme"
      description="Follow your desktop's light/dark setting, or pick one."
    >
      <fieldset className="grid grid-cols-3 gap-2">
        <legend className="sr-only">Theme</legend>
        {options.map(([value, label, Icon]) => (
          <button
            key={value}
            type="button"
            aria-pressed={choice === value}
            onClick={() => setTheme(value)}
            className={cn(
              "flex h-16 flex-col items-center justify-center gap-1.5 rounded-lg border text-sm",
              choice === value
                ? "border-accent bg-accent-soft text-accent-text"
                : "border-border hover:border-border-strong hover:bg-hover",
            )}
          >
            <Icon className="size-4" />
            {label}
          </button>
        ))}
      </fieldset>
    </Section>
  );
}

function GitSettings() {
  const preferences = useApp((state) => state.preferences);

  async function save(patch: Partial<typeof preferences>) {
    try {
      const saved = await api.savePreferences({ ...preferences, ...patch });
      useApp.setState({ preferences: saved });
    } catch (error) {
      reportError(error, "Couldn't save settings");
    }
  }

  return (
    <div className="space-y-6">
      <Section
        title="Check for remote changes"
        description="Rehow only looks. It never pulls, pushes or merges without you confirming."
      >
        <div className="divide-y divide-border rounded-lg border border-border">
          <div className="flex items-center justify-between gap-4 px-4 py-3">
            <span id="sync-on-start-label">
              <span className="block text-sm">When opening a Workspace</span>
              <span className="block text-xs text-muted">
                Check the remote once each time you open Rehow.
              </span>
            </span>
            <Switch
              aria-labelledby="sync-on-start-label"
              checked={preferences.syncOnStart}
              onCheckedChange={(syncOnStart) => void save({ syncOnStart })}
            />
          </div>
          <div className="flex items-center justify-between gap-4 px-4 py-3">
            <label htmlFor="sync-interval">
              <span className="block text-sm">While Rehow is open</span>
              <span className="block text-xs text-muted">
                Check again on a schedule.
              </span>
            </label>
            <Select
              id="sync-interval"
              className="w-44"
              value={String(preferences.syncIntervalMinutes)}
              onChange={(event) =>
                void save({
                  syncIntervalMinutes:
                    event.target.value === "null"
                      ? null
                      : Number(event.target.value),
                })
              }
            >
              {INTERVALS.map(([label, value]) => (
                <option key={label} value={String(value)}>
                  {label}
                </option>
              ))}
            </Select>
          </div>
        </div>
      </Section>
    </div>
  );
}

type Client = "claude-code" | "claude-desktop" | "codex" | "json";
type Access = "readOnly" | "write" | "broad" | "git";

const ACCESS: Array<[Access, string, string]> = [
  ["readOnly", "Read only", "Search and read Guides. Nothing can be changed."],
  [
    "write",
    "Read and edit",
    "Create and edit Guides, Categories, Tags and attachment details.",
  ],
  [
    "broad",
    "Edit and reorganize",
    "Also reorder and remove — each removal still asks you first.",
  ],
  [
    "git",
    "Edit and use Git",
    "Also commit and sync — each one still asks you first.",
  ],
];

function shellQuote(value: string) {
  return /^[\w@%+=:,./-]+$/.test(value)
    ? value
    : `'${value.replaceAll("'", `'\\''`)}'`;
}

function mcpArgs(path: string, access: Access) {
  const args = ["--workspace", `personal=${path}`];
  if (access !== "readOnly") args.push("--allow-write", "personal");
  if (access === "broad") args.push("--allow-broad", "personal");
  if (access === "git") args.push("--allow-git-write", "personal");
  return args;
}

function setupText(
  client: Client,
  binary: string,
  path: string,
  access: Access,
) {
  const args = mcpArgs(path, access);
  const command = [binary, ...args].map(shellQuote).join(" ");
  switch (client) {
    case "claude-code":
      return `claude mcp add rehow -- ${command}`;
    case "codex":
      return `codex mcp add rehow -- ${command}`;
    case "claude-desktop":
    case "json":
      return JSON.stringify(
        { mcpServers: { rehow: { command: binary, args } } },
        null,
        2,
      );
  }
}

const CLIENT_HELP: Record<Client, string> = {
  "claude-code": "Run this in a terminal.",
  codex: "Run this in a terminal.",
  "claude-desktop":
    "Add this to claude_desktop_config.json (Settings → Developer → Edit config), then restart Claude.",
  json: "Use this in any MCP client that accepts a JSON server definition.",
};

function AiSettings() {
  const [info, setInfo] = useState<{
    workspacePath: string;
    mcpBinary: string;
  } | null>(null);
  const [client, setClient] = useState<Client>("claude-code");
  const [access, setAccess] = useState<Access>("readOnly");
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    api.mcpSetup().then(setInfo, (error) => reportError(error));
  }, []);

  const text = info
    ? setupText(client, info.mcpBinary, info.workspacePath, access)
    : "";

  async function copy() {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      toast.error("Couldn't copy to the clipboard");
    }
  }

  return (
    <div className="space-y-6">
      <Section
        title="Connect an AI assistant"
        description="Rehow includes an MCP server so assistants can search and help write your Guides. Anything an assistant writes stays a Draft until you mark it tested yourself."
      >
        <div className="flex flex-wrap gap-1.5">
          {(
            [
              ["claude-code", "Claude Code"],
              ["claude-desktop", "Claude Desktop"],
              ["codex", "Codex"],
              ["json", "Other (JSON)"],
            ] as const
          ).map(([value, label]) => (
            <button
              key={value}
              type="button"
              aria-pressed={client === value}
              onClick={() => setClient(value)}
              className={cn(
                "h-7 rounded-md border px-2.5 text-xs font-medium",
                client === value
                  ? "border-accent bg-accent-soft text-accent-text"
                  : "border-border text-muted hover:border-border-strong hover:text-text",
              )}
            >
              {label}
            </button>
          ))}
        </div>
      </Section>
      <Section title="What can it do?">
        <fieldset className="grid gap-1.5">
          <legend className="sr-only">Assistant access</legend>
          {ACCESS.map(([value, label, detail]) => (
            <button
              key={value}
              type="button"
              aria-pressed={access === value}
              onClick={() => setAccess(value)}
              className={cn(
                "flex items-start gap-3 rounded-lg border px-3 py-2.5 text-left",
                access === value
                  ? "border-accent bg-accent-soft/60"
                  : "border-border hover:bg-hover",
              )}
            >
              <span
                className={cn(
                  "mt-0.5 flex size-4 shrink-0 items-center justify-center rounded-full border",
                  access === value
                    ? "border-accent bg-accent"
                    : "border-border-strong",
                )}
              >
                {access === value ? (
                  <span className="size-1.5 rounded-full bg-accent-fg" />
                ) : null}
              </span>
              <span>
                <span className="block text-sm font-medium">{label}</span>
                <span className="block text-xs text-muted">{detail}</span>
              </span>
            </button>
          ))}
        </fieldset>
      </Section>
      <Section title="Setup" description={CLIENT_HELP[client]}>
        <div className="relative">
          <pre className="max-h-56 overflow-auto rounded-lg border border-border bg-code p-3 pr-20 font-mono text-xs leading-5 break-all whitespace-pre-wrap">
            {info ? text : "Loading…"}
          </pre>
          <Button
            size="sm"
            className="absolute top-2 right-2"
            onClick={() => void copy()}
            disabled={!info}
          >
            {copied ? <Check /> : <Copy />}
            {copied ? "Copied" : "Copy"}
          </Button>
        </div>
        <p className="text-xs text-faint">
          This connects only this Workspace. Run the setup again after moving
          the Workspace folder.
        </p>
      </Section>
    </div>
  );
}
