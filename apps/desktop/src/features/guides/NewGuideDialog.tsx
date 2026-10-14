import { useEffect, useState } from "react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogBody,
  DialogContent,
  DialogFooter,
  DialogHeader,
} from "@/components/ui/dialog";
import { Field, Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { closeDialog, createCategory, createGuide, useApp } from "@/store/app";

const NEW_CATEGORY = "__new__";

export function NewGuideDialog() {
  const open = useApp((state) => state.dialog === "newGuide");
  const workspace = useApp((state) => state.workspace);
  const categoryFilter = useApp((state) => state.categoryFilter);
  const categories =
    workspace?.categories.filter((category) => category.status === "active") ??
    [];

  const [title, setTitle] = useState("");
  const [summary, setSummary] = useState("");
  const [categoryId, setCategoryId] = useState("");
  const [newCategoryName, setNewCategoryName] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // biome-ignore lint/correctness/useExhaustiveDependencies: reset only when the dialog opens.
  useEffect(() => {
    if (!open) return;
    setTitle("");
    setSummary("");
    setError(null);
    setNewCategoryName("");
    const preferred =
      categories.find((category) => category.id === categoryFilter)?.id ??
      categories[0]?.id ??
      NEW_CATEGORY;
    setCategoryId(preferred);
  }, [open]);

  async function submit(event: React.FormEvent) {
    event.preventDefault();
    const cleanTitle = title.trim();
    if (cleanTitle === "") {
      setError("Give the Guide a title.");
      return;
    }
    setBusy(true);
    setError(null);
    let target = categoryId;
    if (target === NEW_CATEGORY) {
      const name = newCategoryName.trim();
      if (name === "") {
        setBusy(false);
        setError("Name the new Category.");
        return;
      }
      const created = await createCategory(name, null);
      if (created === null) {
        setBusy(false);
        return;
      }
      target = created;
    }
    // The file format requires a summary; the title is a sensible start that
    // the user can refine in the editor that opens next.
    const ok = await createGuide({
      title: cleanTitle,
      summary: summary.trim() || cleanTitle,
      categoryId: target,
    });
    setBusy(false);
    if (ok) closeDialog();
  }

  return (
    <Dialog open={open} onOpenChange={(next) => !next && closeDialog()}>
      <DialogContent>
        <form onSubmit={submit}>
          <DialogHeader
            title="New Guide"
            description="Give it a name you'd search for later. You'll write the steps next."
          />
          <DialogBody className="grid gap-4 pb-5">
            <Field label="Title" htmlFor="new-guide-title" error={error}>
              <Input
                id="new-guide-title"
                autoFocus
                value={title}
                onChange={(event) => setTitle(event.target.value)}
                placeholder="e.g. Set up WireGuard on a new laptop"
                maxLength={200}
              />
            </Field>
            <Field
              label="Summary"
              htmlFor="new-guide-summary"
              hint="Optional — one line about when you'd need this."
            >
              <Input
                id="new-guide-summary"
                value={summary}
                onChange={(event) => setSummary(event.target.value)}
                maxLength={500}
              />
            </Field>
            <Field label="Category" htmlFor="new-guide-category">
              <Select
                id="new-guide-category"
                value={categoryId}
                onChange={(event) => setCategoryId(event.target.value)}
              >
                {categories.map((category) => (
                  <option key={category.id} value={category.id}>
                    {category.name}
                  </option>
                ))}
                <option value={NEW_CATEGORY}>+ New Category…</option>
              </Select>
            </Field>
            {categoryId === NEW_CATEGORY ? (
              <Field label="New Category name" htmlFor="new-guide-new-category">
                <Input
                  id="new-guide-new-category"
                  value={newCategoryName}
                  onChange={(event) => setNewCategoryName(event.target.value)}
                  placeholder="e.g. Networking"
                  maxLength={200}
                />
              </Field>
            ) : null}
          </DialogBody>
          <DialogFooter>
            <Button variant="ghost" onClick={closeDialog}>
              Cancel
            </Button>
            <Button variant="primary" type="submit" disabled={busy}>
              {busy ? "Creating…" : "Create and edit"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
