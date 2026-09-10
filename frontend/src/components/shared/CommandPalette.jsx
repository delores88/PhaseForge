import { useEffect, useMemo, useRef, useState } from "react";
import { useRouter } from "next/router";
import {
  Activity,
  ArrowRight,
  Atom,
  Cpu,
  FlaskConical,
  Search,
  Settings,
  Wallet,
} from "lucide-react";

const commands = [
  { label: "Usage & cost", detail: "Budgets, tokens, and stopping agent calls", href: "/usage/", icon: Wallet },
  { label: "Open laboratory", detail: "Research workspace", href: "/", icon: Atom },
  { label: "View run queue", detail: "History and progress", href: "/runs/", icon: Activity },
  { label: "Inspect compute", detail: "CPU, GPU and scheduler", href: "/hardware/", icon: Cpu },
  { label: "Configure AI providers", detail: "OpenAI and Anthropic", href: "/settings/", icon: Settings },
];

export default function CommandPalette({ open, onClose, onNewQuestion }) {
  const router = useRouter();
  const inputRef = useRef(null);
  const [query, setQuery] = useState("");

  useEffect(() => {
    if (!open) return;
    setQuery("");
    window.setTimeout(() => inputRef.current?.focus(), 0);
  }, [open]);

  const filtered = useMemo(() => {
    const normalized = query.trim().toLowerCase();
    if (!normalized) return commands;
    return commands.filter(
      (command) =>
        command.label.toLowerCase().includes(normalized) ||
        command.detail.toLowerCase().includes(normalized),
    );
  }, [query]);

  if (!open) return null;

  return (
    <div className="paletteBackdrop" onMouseDown={onClose}>
      <section
        className="commandPalette"
        role="dialog"
        aria-modal="true"
        aria-label="Command palette"
        onMouseDown={(event) => event.stopPropagation()}
      >
        <div className="commandPalette__input">
          <Search size={17} />
          <input
            ref={inputRef}
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Go to a view or start work…"
            onKeyDown={(event) => {
              if (event.key === "Escape") onClose();
              if (event.key === "Enter" && filtered[0]) {
                router.push(filtered[0].href);
                onClose();
              }
            }}
          />
          <kbd>Esc</kbd>
        </div>

        <button className="commandPalette__primary" onClick={onNewQuestion}>
          <span className="commandPalette__icon">
            <FlaskConical size={17} />
          </span>
          <span>
            <strong>Start a research question</strong>
            <small>Create a project and build a runnable experiment</small>
          </span>
          <ArrowRight size={17} />
        </button>

        <div className="commandPalette__groupLabel">Navigate</div>
        <div className="commandPalette__results">
          {filtered.map(({ label, detail, href, icon: Icon }) => (
            <button
              key={href}
              onClick={() => {
                router.push(href);
                onClose();
              }}
            >
              <Icon size={17} />
              <span>
                <strong>{label}</strong>
                <small>{detail}</small>
              </span>
              <ArrowRight size={15} />
            </button>
          ))}
          {!filtered.length && (
            <div className="emptySearch">No matching command.</div>
          )}
        </div>
      </section>
    </div>
  );
}
