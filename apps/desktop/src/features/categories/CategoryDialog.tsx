import { useEffect, useState } from "react";
import { create } from "zustand";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogBody,
  DialogContent,
  DialogFooter,
  DialogHeader,
} from "@/components/ui/dialog";
import { Field, Input } from "@/components/ui/input";
import {
  createCategory,
  filterByCategory,
  renameCategory,
  useApp,
} from "@/store/app";

/** `categoryId === null` creates a new Category; a string edits that one. */
export const useCategoryDialog = create<{
  editing: string | null | undefined;
  open: (categoryId: string | null) => void;
  close: () => void;
}>((set) => ({
  editing: undefined,
  open: (categoryId) => set({ editing: categoryId }),
  close: () => set({ editing: undefined }),
}));

export function CategoryDialog() {
  const editing = useCategoryDialog((state) => state.editing);
  const close = useCategoryDialog((state) => state.close);
  const category = useApp((state) =>
    editing
      ? state.workspace?.categories.find((item) => item.id === editing)
      : undefined,
  );
  const open = editing !== undefined;
  const [name, setName] = useState("");
  const [summary, setSummary] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!open) return;
    setName(category?.name ?? "");
    setSummary(category?.summary ?? "");
    setError(null);
  }, [open, category]);

  async function submit(event: React.FormEvent) {
    event.preventDefault();
    const cleanName = name.trim();
    if (cleanName === "") {
      setError("Give the Category a name.");
      return;
    }
    const cleanSummary = summary.trim() === "" ? null : summary.trim();
    setBusy(true);
    if (editing) {
      if (await renameCategory(editing, cleanName, cleanSummary)) close();
    } else {
      const created = await createCategory(cleanName, cleanSummary);
      if (created) {
        close();
        filterByCategory(created);
      }
    }
    setBusy(false);
  }

  return (
    <Dialog open={open} onOpenChange={(next) => !next && close()}>
      <DialogContent className="w-[min(92vw,440px)]">
        <form onSubmit={submit}>
          <DialogHeader
            title={editing ? "Edit Category" : "New Category"}
            description={
              editing ? undefined : "Categories are folders in your Workspace."
            }
          />
          <DialogBody className="grid gap-4 pb-5">
            <Field label="Name" htmlFor="category-name" error={error}>
              <Input
                id="category-name"
                autoFocus
                value={name}
                onChange={(event) => setName(event.target.value)}
                placeholder="e.g. Docker & Containers"
                maxLength={200}
              />
            </Field>
            <Field
              label="Description"
              htmlFor="category-summary"
              hint="Optional"
            >
              <Input
                id="category-summary"
                value={summary}
                onChange={(event) => setSummary(event.target.value)}
                maxLength={500}
              />
            </Field>
          </DialogBody>
          <DialogFooter>
            <Button variant="ghost" onClick={close}>
              Cancel
            </Button>
            <Button variant="primary" type="submit" disabled={busy}>
              {editing ? "Save" : "Create Category"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
