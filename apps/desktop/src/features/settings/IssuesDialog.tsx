import { FileWarning } from "lucide-react";
import type { WorkspaceIssueKind } from "@/api/payloads";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogBody,
  DialogContent,
  DialogFooter,
  DialogHeader,
} from "@/components/ui/dialog";
import { plural } from "@/lib/format";
import { closeDialog, refreshWorkspace, useApp } from "@/store/app";

/** Plain-language explanations for files Rehow skipped while loading. */
export const ISSUE_TEXT: Record<
  WorkspaceIssueKind,
  { title: string; fix: string }
> = {
  unsafeEntry: {
    title: "Skipped for safety",
    fix: "It's a link, special file, or has unusual permissions. Replace it with a normal file, or move it out of the Workspace.",
  },
  unsupportedEntry: {
    title: "Not in a place Rehow reads",
    fix: "Guides must be .md files with lowercase names (letters, numbers, dashes) inside a Category folder.",
  },
  missingCategoryDescriptor: {
    title: "Category folder without category.yaml",
    fix: "This can happen if creating a Category was interrupted. Restore category.yaml from Git or a backup, or remove the empty folder.",
  },
  malformedCategory: {
    title: "Category file couldn't be read",
    fix: "Its category.yaml has invalid or missing fields. Fix it in a text editor or restore it from Git.",
  },
  malformedGuide: {
    title: "Guide couldn't be read",
    fix: "Its frontmatter (the --- block at the top) has invalid or missing fields. Fix it in a text editor or restore it from Git.",
  },
  fileTooLarge: {
    title: "File too large",
    fix: "Rehow skips very large files. Split it up or move it outside the Workspace.",
  },
  categoryMismatch: {
    title: "Guide is in the wrong Category folder",
    fix: "Its category_id doesn't match the folder it's in. Move it back, or fix category_id.",
  },
  duplicateCategoryId: {
    title: "Two Categories share an ID",
    fix: "Usually caused by copying a Category folder. Give one of them a new id in category.yaml.",
  },
  duplicateGuideId: {
    title: "Two Guides share an ID",
    fix: "Usually caused by copying a Guide file. Give one of them a new id in its frontmatter.",
  },
  verificationMismatch: {
    title: "Tested Guide was changed outside Rehow",
    fix: "It's shown as “Needs retest”. Follow it again, then mark it as tested.",
  },
  missingAttachment: {
    title: "Attachment file is missing",
    fix: "Restore it from Git or a backup, or remove the attachment from the Guide.",
  },
  orphanAttachment: {
    title: "Attachment not used by any Guide",
    fix: "Attach it to a Guide again, or move it out of the attachments folder.",
  },
  attachmentHashMismatch: {
    title: "Attachment content changed",
    fix: "The file no longer matches what was attached. Restore the original or attach the new file again.",
  },
  attachmentSizeMismatch: {
    title: "Attachment size changed",
    fix: "The file no longer matches what was attached. Restore the original or attach the new file again.",
  },
};

/** Plain-language reasons for core Guide diagnostics. Unknown codes fall back. */
const DIAGNOSTIC_TEXT: Record<string, string> = {
  fileTooLarge: "The file is too large.",
  frontmatterTooLarge: "The metadata block at the top is too large.",
  invalidUtf8: "The file isn't valid UTF-8 text.",
  unsupportedLineEndings: "The metadata block mixes line endings.",
  misplacedByteOrderMark: "There's a stray byte-order mark inside the file.",
  missingFrontmatter: "The metadata block at the top is missing.",
  missingClosingDelimiter: "The metadata block has no closing --- line.",
  yamlSyntax: "The metadata block isn't valid YAML.",
  unsupportedYamlFeature:
    "The metadata uses a YAML feature Rehow doesn't allow (anchors, tags or several documents).",
  invalidUnicode: "The metadata contains a character Rehow doesn't allow.",
  frontmatterNotMapping: "The metadata block isn't a list of key: value pairs.",
  nonStringKey: "A metadata key isn't plain text.",
  duplicateKey: "A metadata key appears twice.",
  missingRequiredField: "A required field is missing.",
  invalidFieldType: "A field has the wrong kind of value.",
  invalidFieldValue: "A field has a value Rehow can't accept.",
  duplicateFieldValue: "A list field repeats the same value.",
  resourceLimit: "The metadata is too complex to read safely.",
};

function diagnosticLine(issue: {
  diagnostic: string | null;
  field: string | null;
  line: number | null;
  column: number | null;
}) {
  if (issue.diagnostic === null) return null;
  const reason =
    DIAGNOSTIC_TEXT[issue.diagnostic] ?? "The file couldn't be read.";
  const where =
    issue.line !== null
      ? ` Line ${issue.line}${issue.column !== null ? `, column ${issue.column}` : ""}.`
      : "";
  const field = issue.field !== null ? ` Field: ${issue.field}.` : "";
  return `${reason}${field}${where}`;
}

export function IssuesDialog() {
  const open = useApp((state) => state.dialog === "issues");
  const workspace = useApp((state) => state.workspace);
  if (workspace === null) return null;

  return (
    <Dialog open={open} onOpenChange={(next) => !next && closeDialog()}>
      <DialogContent className="w-[min(92vw,640px)]">
        <DialogHeader
          title={`${plural(workspace.issueCount, "file")} skipped`}
          description="Everything else loaded normally. Rehow never changes these files on its own."
        />
        <DialogBody className="pb-4">
          <ul className="divide-y divide-border rounded-lg border border-border">
            {workspace.issues.map((issue) => {
              const text = ISSUE_TEXT[issue.kind];
              return (
                <li
                  key={`${issue.kind}:${issue.relativePath}`}
                  className="flex gap-3 px-4 py-3"
                >
                  <FileWarning className="mt-0.5 size-4 shrink-0 text-warning" />
                  <div className="min-w-0 space-y-1">
                    <p className="text-sm font-medium">{text.title}</p>
                    <p className="font-mono text-xs break-all text-muted">
                      {issue.relativePath}
                    </p>
                    {diagnosticLine(issue) ? (
                      <p className="text-xs font-medium text-text">
                        {diagnosticLine(issue)}
                      </p>
                    ) : null}
                    <p className="text-xs text-muted">{text.fix}</p>
                  </div>
                </li>
              );
            })}
          </ul>
          {workspace.issuesTruncated ? (
            <p className="mt-2 text-xs text-faint">
              Showing the first {workspace.issues.length} of{" "}
              {workspace.issueCount}.
            </p>
          ) : null}
        </DialogBody>
        <DialogFooter>
          <Button variant="ghost" onClick={closeDialog}>
            Close
          </Button>
          <Button
            onClick={() => {
              closeDialog();
              void refreshWorkspace();
            }}
          >
            Check again
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
