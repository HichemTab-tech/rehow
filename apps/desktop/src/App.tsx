import { Toaster } from "sonner";
import { ConfirmHost } from "@/components/ui/confirm-host";
import { TooltipProvider } from "@/components/ui/tooltip";
import { CommandPalette } from "@/features/palette/CommandPalette";
import { useAppEffects } from "@/features/shell/useAppEffects";
import { Workbench } from "@/features/shell/Workbench";
import { Welcome } from "@/features/welcome/Welcome";
import { useTheme } from "@/lib/theme";
import { useApp } from "@/store/app";

export const APPLICATION_NAME = "Rehow";

export function App() {
  useAppEffects();
  const theme = useTheme((state) => state.resolved);
  const booting = useApp((state) => state.booting);
  const hasWorkspace = useApp((state) => state.workspace !== null);

  return (
    <TooltipProvider delayDuration={500} skipDelayDuration={200}>
      <a
        href="#guide-list"
        className="sr-only focus:not-sr-only focus:fixed focus:top-2 focus:left-2 focus:z-50 focus:rounded-md focus:bg-panel focus:px-3 focus:py-2 focus:shadow-menu"
      >
        Skip to Guides
      </a>
      {booting ? (
        <div className="h-full" aria-busy="true" />
      ) : hasWorkspace ? (
        <Workbench />
      ) : (
        <Welcome />
      )}
      <CommandPalette />
      <ConfirmHost />
      <Toaster
        theme={theme}
        position="bottom-right"
        visibleToasts={4}
        offset={{ bottom: 36, right: 16 }}
        closeButton
        toastOptions={{
          classNames: {
            toast:
              "!rounded-lg !border !border-border !bg-raised !text-text !shadow-menu !font-sans",
            description: "!text-muted",
            actionButton: "!bg-accent !text-accent-fg",
          },
        }}
      />
    </TooltipProvider>
  );
}
