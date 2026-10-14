// Theme preference: follow the system by default, or force light/dark.
// Stored locally because it is a per-device display preference.
import { create } from "zustand";

export type ThemeChoice = "system" | "light" | "dark";

const KEY = "rehow:theme";
const media =
  typeof window !== "undefined"
    ? window.matchMedia("(prefers-color-scheme: dark)")
    : null;

function readChoice(): ThemeChoice {
  try {
    const stored = localStorage.getItem(KEY);
    if (stored === "light" || stored === "dark" || stored === "system")
      return stored;
  } catch {
    // Use the default.
  }
  const param = new URLSearchParams(location.search).get("theme");
  return param === "light" || param === "dark" ? param : "system";
}

function resolve(choice: ThemeChoice): "light" | "dark" {
  if (choice !== "system") return choice;
  return media?.matches ? "dark" : "light";
}

function apply(choice: ThemeChoice) {
  const root = document.documentElement;
  if (choice === "system") root.removeAttribute("data-theme");
  else root.setAttribute("data-theme", choice);
}

type ThemeState = { choice: ThemeChoice; resolved: "light" | "dark" };

const initialChoice = typeof window !== "undefined" ? readChoice() : "system";
if (typeof document !== "undefined") apply(initialChoice);

export const useTheme = create<ThemeState>(() => ({
  choice: initialChoice,
  resolved: typeof window !== "undefined" ? resolve(initialChoice) : "light",
}));

media?.addEventListener("change", () => {
  const { choice } = useTheme.getState();
  useTheme.setState({ resolved: resolve(choice) });
});

export function setTheme(choice: ThemeChoice) {
  try {
    localStorage.setItem(KEY, choice);
  } catch {
    // Preference simply won't persist.
  }
  apply(choice);
  useTheme.setState({ choice, resolved: resolve(choice) });
}
