import { ArrowRight, FlaskConical, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";

export default function NewWorldDialog({ open, onClose, onCreate, busy }) {
  const [question, setQuestion] = useState("");
  const [name, setName] = useState("");
  const inputRef = useRef(null);
  useEffect(() => { if (open) window.setTimeout(() => inputRef.current?.focus(), 40); }, [open]);
  if (!open) return null;
  const submit = async (event) => {
    event.preventDefault();
    if (question.trim().length < 8 || busy) return;
    try {
      await onCreate({ question: question.trim(), name: name.trim() || null });
      setQuestion(""); setName(""); onClose();
    } catch { /* Parent displays the creation error; preserve entered text. */ }
  };
  return (
    <div className="modalBackdrop" onMouseDown={onClose} role="presentation">
      <form className="newWorldDialog" onSubmit={submit} onMouseDown={(event) => event.stopPropagation()} role="dialog" aria-modal="true" aria-labelledby="new-world-title">
        <header><span className="newWorldDialog__icon"><FlaskConical size={22} /></span><div><strong id="new-world-title">New project</strong><p>This creates a local research world only. No paid model call is made until you send a message in chat.</p></div>{<button type="button" className="iconButton" onClick={onClose} aria-label="Close new research world dialog"><X size={17} /></button>}</header>
        <label><span>Scientific question</span><textarea ref={inputRef} value={question} onChange={(event) => setQuestion(event.target.value)} rows={5} placeholder="What behavior, mechanism, or candidate relationship should the laboratory investigate?" /></label>
        <label><span>World name <em>optional</em></span><input type="text" value={name} onChange={(event) => setName(event.target.value)} placeholder="Derived from the question when blank" /></label>
        <div className="newWorldBoundary"><strong>Start with the question; choose a runnable scope.</strong><span>You can explore supported models with declared assumptions. Missing engines stay explicit; no forced research-plan checklist.</span></div>
        <footer><button type="submit" className="button button--primary button--large" disabled={busy || question.trim().length < 8}>Create project <ArrowRight size={15} /></button></footer>
      </form>
    </div>
  );
}
