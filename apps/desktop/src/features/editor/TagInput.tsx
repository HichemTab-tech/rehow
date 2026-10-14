import { X } from "lucide-react";
import { useId, useMemo, useState } from "react";
import { cn } from "@/lib/utils";

/** Chip-style input for Tags and aliases, with suggestions from existing values. */
export function TagInput({
  value,
  onChange,
  suggestions = [],
  placeholder,
  ariaLabel,
  prefix,
  maxLength,
}: {
  value: string[];
  onChange: (value: string[]) => void;
  suggestions?: string[];
  placeholder: string;
  ariaLabel: string;
  prefix?: string;
  maxLength: number;
}) {
  const [text, setText] = useState("");
  const [open, setOpen] = useState(false);
  const [highlight, setHighlight] = useState(0);
  const [navigated, setNavigated] = useState(false);
  const listId = useId();

  const matches = useMemo(() => {
    const query = text.trim().toLocaleLowerCase();
    return suggestions
      .filter(
        (item) =>
          !value.includes(item) &&
          (query === "" || item.toLocaleLowerCase().includes(query)),
      )
      .slice(0, 6);
  }, [suggestions, text, value]);

  function add(raw: string) {
    const items = raw
      .split(",")
      .map((item) => item.trim())
      .filter(
        (item) =>
          item !== "" && [...item].length <= maxLength && !value.includes(item),
      );
    if (items.length > 0) onChange([...value, ...items].slice(0, 64));
    setText("");
    setHighlight(0);
  }

  const showList = open && matches.length > 0;

  return (
    <div className="flex min-h-8 flex-wrap items-center gap-1 rounded-md border border-border-strong bg-panel px-1.5 py-1 focus-within:border-ring focus-within:shadow-[0_0_0_3px_color-mix(in_srgb,var(--ring)_22%,transparent)]">
      {value.map((item) => (
        <span
          key={item}
          className="inline-flex h-6 items-center gap-0.5 rounded bg-hover pr-0.5 pl-1.5 text-xs text-text"
        >
          {prefix ? <span className="text-faint">{prefix}</span> : null}
          {item}
          <button
            type="button"
            aria-label={`Remove ${item}`}
            onClick={() =>
              onChange(value.filter((existing) => existing !== item))
            }
            className="inline-flex size-5 items-center justify-center rounded text-faint hover:bg-active hover:text-text"
          >
            <X className="size-3" />
          </button>
        </span>
      ))}
      <div className="relative min-w-24 flex-1">
        <input
          value={text}
          aria-label={ariaLabel}
          aria-expanded={showList}
          aria-controls={listId}
          role="combobox"
          aria-autocomplete="list"
          aria-activedescendant={
            showList ? `${listId}-${highlight}` : undefined
          }
          placeholder={value.length === 0 ? placeholder : ""}
          onChange={(event) => {
            setText(event.target.value);
            setOpen(true);
            setHighlight(0);
            setNavigated(false);
          }}
          onFocus={() => setOpen(true)}
          onBlur={() => {
            setOpen(false);
            if (text.trim()) add(text);
          }}
          onKeyDown={(event) => {
            if (event.key === "Enter" || event.key === ",") {
              event.preventDefault();
              // Use the highlighted suggestion only when the user picked it.
              const picked =
                showList && (text.trim() === "" || navigated)
                  ? matches[highlight]
                  : undefined;
              add(picked ?? text);
              setNavigated(false);
            } else if (
              event.key === "Backspace" &&
              text === "" &&
              value.length > 0
            ) {
              onChange(value.slice(0, -1));
            } else if (event.key === "ArrowDown" && showList) {
              event.preventDefault();
              setHighlight((highlight + 1) % matches.length);
              setNavigated(true);
            } else if (event.key === "ArrowUp" && showList) {
              event.preventDefault();
              setHighlight((highlight - 1 + matches.length) % matches.length);
              setNavigated(true);
            } else if (event.key === "Escape" && showList) {
              event.stopPropagation();
              setOpen(false);
            }
          }}
          className="h-6 w-full bg-transparent px-1 text-sm outline-none placeholder:text-faint"
        />
        {showList ? (
          <div
            id={listId}
            role="listbox"
            className="absolute top-7 left-0 z-20 min-w-44 rounded-lg border border-border bg-raised p-1 shadow-menu"
          >
            {matches.map((item, index) => (
              // biome-ignore lint/a11y/useFocusableInteractive: combobox options stay unfocused; the input owns focus via aria-activedescendant.
              <div
                key={item}
                id={`${listId}-${index}`}
                role="option"
                aria-selected={index === highlight}
                onMouseDown={(event) => {
                  event.preventDefault();
                  add(item);
                }}
                onMouseEnter={() => setHighlight(index)}
                className={cn(
                  "flex h-7 items-center rounded-md px-2 text-sm",
                  index === highlight && "bg-hover",
                )}
              >
                {prefix ? <span className="text-faint">{prefix}</span> : null}
                {item}
              </div>
            ))}
          </div>
        ) : null}
      </div>
    </div>
  );
}
