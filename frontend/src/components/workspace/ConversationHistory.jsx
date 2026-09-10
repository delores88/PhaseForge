import { Check, MoreHorizontal, Pencil, Plus, Search, Trash2, X } from "lucide-react";
import { useMemo, useState } from "react";
import { formatDate } from "@/lib/format";

export default function ConversationHistory({
  projects,
  activeId,
  onSelect,
  onNew,
  onRename,
  onDelete,
}) {
  const [query, setQuery] = useState("");
  const [editing, setEditing] = useState(null);
  const [name, setName] = useState("");
  const filtered = useMemo(() => {
    const needle = query.trim().toLowerCase();
    return needle
      ? projects.filter((project) =>
          `${project.name} ${project.question}`.toLowerCase().includes(needle),
        )
      : projects;
  }, [projects, query]);

  return (
    <div className="conversationHistory">
      <header>
        <strong>Research worlds</strong>
        <button type="button" onClick={onNew}><Plus size={14} />New</button>
      </header>
      <label className="conversationSearch">
        <Search size={13} />
        <input
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          placeholder="Search conversations"
        />
      </label>
      <div className="conversationList">
        {filtered.map((project) => (
          <article
            key={project.id}
            className={project.id === activeId ? "active" : ""}
          >
            {editing === project.id ? (
              <form
                onSubmit={(event) => {
                  event.preventDefault();
                  if (name.trim()) onRename(project.id, name.trim());
                  setEditing(null);
                }}
              >
                <input value={name} onChange={(event) => setName(event.target.value)} autoFocus />
                <button title="Save"><Check size={13} /></button>
                <button type="button" onClick={() => setEditing(null)} title="Cancel"><X size={13} /></button>
              </form>
            ) : (
              <>
                <button type="button" className="conversationMain" onClick={() => onSelect(project.id)}>
                  <strong>{project.name}</strong>
                  <small>{formatDate(project.updated_at)}</small>
                </button>
                <details>
                  <summary aria-label="Conversation actions"><MoreHorizontal size={14} /></summary>
                  <div>
                    <button type="button" onClick={() => { setEditing(project.id); setName(project.name); }}><Pencil size={12} />Rename</button>
                    <button type="button" className="danger" onClick={() => onDelete(project.id)}><Trash2 size={12} />Delete</button>
                  </div>
                </details>
              </>
            )}
          </article>
        ))}
        {!filtered.length && <p className="conversationEmpty">No matching research worlds.</p>}
      </div>
    </div>
  );
}
