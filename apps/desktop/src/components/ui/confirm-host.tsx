import * as AlertDialog from "@radix-ui/react-alert-dialog";
import { settleConfirm, useConfirmStore } from "@/lib/confirm";
import { Button } from "./button";

export function ConfirmHost() {
  const request = useConfirmStore((state) => state.request);
  return (
    <AlertDialog.Root
      open={request !== null}
      onOpenChange={(open) => !open && settleConfirm(false)}
    >
      <AlertDialog.Portal>
        <AlertDialog.Overlay className="fixed inset-0 z-50 bg-black/30 data-[state=open]:animate-[fade-in_120ms_ease-out] dark:bg-black/50" />
        <AlertDialog.Content className="fixed top-[24vh] left-1/2 z-50 w-[min(92vw,420px)] -translate-x-1/2 rounded-xl border border-border bg-raised p-5 shadow-dialog outline-none data-[state=open]:animate-[dialog-in_140ms_cubic-bezier(0.2,0.9,0.3,1)]">
          <AlertDialog.Title className="text-[15px] font-semibold tracking-tight">
            {request?.title}
          </AlertDialog.Title>
          <AlertDialog.Description className="mt-1.5 text-sm text-muted">
            {request?.description ?? ""}
          </AlertDialog.Description>
          <div className="mt-5 flex justify-end gap-2">
            <AlertDialog.Cancel asChild>
              <Button variant="secondary">
                {request?.cancelLabel ?? "Cancel"}
              </Button>
            </AlertDialog.Cancel>
            <AlertDialog.Action asChild>
              <Button
                variant={request?.destructive ? "danger" : "primary"}
                onClick={() => settleConfirm(true)}
              >
                {request?.confirmLabel ?? "Continue"}
              </Button>
            </AlertDialog.Action>
          </div>
        </AlertDialog.Content>
      </AlertDialog.Portal>
    </AlertDialog.Root>
  );
}
