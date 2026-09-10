import { Braces, Play, Upload, X } from "lucide-react";
import { useState } from "react";

export default function ImportManifestDialog({ open, onClose, onImport, busy }) {
  const [text, setText] = useState("");
  const [autoRun, setAutoRun] = useState(false);
  const [error, setError] = useState(null);
  if (!open) return null;
  const submit = async (event) => {
    event.preventDefault();
    try {
      setError(null);
      const manifest = JSON.parse(text);
      await onImport({ manifest, auto_run: autoRun });
      setText("");
      onClose();
    } catch (value) {
      setError(value.message || "Manifest JSON is invalid.");
    }
  };
  return (
    <div className="modalBackdrop" onMouseDown={onClose} role="presentation">
      <form className="manifestImportDialog" onSubmit={submit} onMouseDown={(event) => event.stopPropagation()} role="dialog" aria-modal="true" aria-labelledby="import-manifest-title">
        <header><div><Braces size={18} /><span><strong id="import-manifest-title">Import generic manifest</strong><small>The Rust validator remains authoritative.</small></span></div><button type="button" className="iconButton" onClick={onClose} aria-label="Close manifest import dialog"><X size={17} /></button></header>
        <textarea value={text} onChange={(event) => setText(event.target.value)} placeholder="Paste an ExperimentManifestDraft JSON object…" spellCheck="false" />
        {error && <div className="chatError">{error}</div>}
        <footer><label className="compactToggle"><input type="checkbox" checked={autoRun} onChange={(event) => setAutoRun(event.target.checked)} /><span>Run after validation</span></label><button type="submit" className="button button--primary" disabled={busy || !text.trim()}>{autoRun ? <Play size={14} /> : <Upload size={14} />} Validate and import</button></footer>
      </form>
    </div>
  );
}
