import {
  Dialog,
  DialogBody,
  DialogContent,
  DialogHeader,
} from "@/components/ui/dialog";
import { Kbd } from "@/components/ui/input";
import { modKey } from "@/lib/format";
import { closeDialog, useApp } from "@/store/app";

const GROUPS: Array<[string, Array<[string, string[]]>]> = [
  [
    "Anywhere",
    [
      ["Search or run a command", [modKey, "K"]],
      ["Focus the Guide search", ["/"]],
      ["New Guide", [modKey, "N"]],
      ["Toggle sidebar", [modKey, "B"]],
      ["Reload from disk", [modKey, "R"]],
      ["Settings", [modKey, ","]],
      ["Show shortcuts", ["?"]],
    ],
  ],
  [
    "Reading",
    [
      ["Next / previous Guide", ["J", "K"]],
      ["Edit Guide", ["E"]],
    ],
  ],
  [
    "Editing",
    [
      ["Save", [modKey, "S"]],
      ["Save and close the editor", [modKey, "Shift", "S"]],
      ["Find in Guide", [modKey, "F"]],
      ["Undo / redo", [modKey, "Z"]],
    ],
  ],
];

export function ShortcutsDialog() {
  const open = useApp((state) => state.dialog === "shortcuts");
  return (
    <Dialog open={open} onOpenChange={(next) => !next && closeDialog()}>
      <DialogContent className="w-[min(92vw,460px)]">
        <DialogHeader title="Keyboard shortcuts" />
        <DialogBody className="space-y-5 pb-5">
          {GROUPS.map(([title, items]) => (
            <section key={title}>
              <h3 className="mb-1.5 text-2xs font-semibold text-faint uppercase tracking-[0.06em]">
                {title}
              </h3>
              <ul className="divide-y divide-border">
                {items.map(([label, keys]) => (
                  <li
                    key={label}
                    className="flex h-8 items-center justify-between text-sm"
                  >
                    {label}
                    <span className="flex gap-1">
                      {keys.map((key) => (
                        <Kbd key={key}>{key}</Kbd>
                      ))}
                    </span>
                  </li>
                ))}
              </ul>
            </section>
          ))}
        </DialogBody>
      </DialogContent>
    </Dialog>
  );
}
