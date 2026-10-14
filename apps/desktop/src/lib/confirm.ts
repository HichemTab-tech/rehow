// Promise-based confirmation dialogs, rendered by <ConfirmHost />.
import { create } from "zustand";

export type ConfirmOptions = {
  title: string;
  description?: string;
  confirmLabel?: string;
  cancelLabel?: string;
  destructive?: boolean;
};

type ConfirmState = {
  request: (ConfirmOptions & { resolve: (value: boolean) => void }) | null;
};

export const useConfirmStore = create<ConfirmState>(() => ({ request: null }));

export function confirm(options: ConfirmOptions): Promise<boolean> {
  return new Promise((resolve) => {
    useConfirmStore.getState().request?.resolve(false);
    useConfirmStore.setState({ request: { ...options, resolve } });
  });
}

export function settleConfirm(value: boolean) {
  const { request } = useConfirmStore.getState();
  if (!request) return;
  useConfirmStore.setState({ request: null });
  request.resolve(value);
}
