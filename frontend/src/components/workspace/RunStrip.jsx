import { Ban, CheckCircle2, Clock3, LoaderCircle, XCircle } from "lucide-react";
import { formatDate, statusLabel } from "@/lib/format";

const icons = {
  completed: CheckCircle2,
  failed: XCircle,
  running: LoaderCircle,
  queued: Clock3,
  cancelled: Ban,
};

export default function RunStrip({ runs, selectedRunId, onSelect, onCancel }) {
  if (!runs.length) return null;
  return (
    <section className="runStrip">
      <header><strong>Run lineage</strong><span>{runs.length} retained</span></header>
      <div className="runStrip__scroll">
        {runs.map((run) => {
          const Icon = icons[run.status] || Clock3;
          const cancellable = ["queued", "running"].includes(run.status);
          return (
            <article
              className={`runCard ${selectedRunId === run.id ? "runCard--active" : ""}`}
              key={run.id}
            >
              <button
                type="button"
                className="runCard__select"
                onClick={() => onSelect(run)}
                aria-label={`Open run ${run.name}`}
              >
                <span className={`runCard__status runCard__status--${run.status}`}>
                  <Icon className={run.status === "running" ? "spin" : ""} size={14} />
                </span>
                <span className="runCard__body">
                  <strong>{run.name}</strong>
                  <small>{statusLabel(run.status)} · {formatDate(run.queued_at)}</small>
                </span>
              </button>
              {cancellable && (
                <button
                  type="button"
                  className="runCard__cancel"
                  title="Cancel run"
                  aria-label={`Cancel run ${run.name}`}
                  onClick={() => onCancel(run.id)}
                >
                  <Ban size={13} />
                </button>
              )}
              {cancellable && (
                <i
                  className="runCard__progress"
                  style={{ width: `${(run.progress || 0) * 100}%` }}
                />
              )}
            </article>
          );
        })}
        {!runs.length && (
          <div className="runStrip__empty">
            Runs will appear here with their exact manifest revision.
          </div>
        )}
      </div>
    </section>
  );
}
