import Link from "next/link";
import {
  Activity,
  Ban,
  CheckCircle2,
  Clock3,
  ExternalLink,
  LoaderCircle,
  RefreshCw,
  Search,
  XCircle,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { api } from "@/lib/api";
import { ErrorState, LoadingState } from "@/components/shared/AsyncState";
import { formatDate, statusLabel } from "@/lib/format";

const statusIcons = {
  completed: CheckCircle2,
  failed: XCircle,
  running: LoaderCircle,
  queued: Clock3,
  cancelled: Ban,
};

export default function RunsView({ backend, onHardware, onEventState }) {
  const [runs, setRuns] = useState([]);
  const [filter, setFilter] = useState("all");
  const [query, setQuery] = useState("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(null);

  const load = useCallback(async () => {
    if (!backend.connected) return;
    try {
      setError(null);
      const [runResponse, hardware] = await Promise.all([api.runs(500), api.hardware()]);
      setRuns(runResponse.runs || []);
      onHardware?.(hardware);
    } catch (value) {
      setError(value.message);
    } finally {
      setLoading(false);
    }
  }, [backend.connected, onHardware]);

  useEffect(() => {
    load();
    const interval = window.setInterval(load, 1800);
    return () => window.clearInterval(interval);
  }, [load]);
  useEffect(() => {
    onEventState?.(backend.connected ? "polling" : "disconnected");
  }, [backend.connected, onEventState]);

  const filtered = useMemo(() => {
    const normalized = query.trim().toLowerCase();
    return runs.filter((run) => {
      const statusMatch = filter === "all" || run.status === filter;
      const queryMatch = !normalized
        || run.name.toLowerCase().includes(normalized)
        || run.capability_id.toLowerCase().includes(normalized)
        || run.phase.toLowerCase().includes(normalized)
        || run.manifest_id.toLowerCase().includes(normalized);
      return statusMatch && queryMatch;
    });
  }, [filter, query, runs]);

  const cancel = async (id) => {
    try {
      const updated = await api.cancelRun(id);
      setRuns((current) => current.map((run) => run.id === id ? updated : run));
    } catch (value) {
      setError(value.message);
    }
  };

  if (loading && backend.connected) return <LoadingState label="Loading experiment history…" />;

  return (
    <section className="runsView">
      <div className="viewControls">
        <div className="searchBox"><Search size={15} /><input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="Search runs, capabilities, or manifest IDs" /></div>
        <div className="segmentedControl">
          {["all", "running", "queued", "completed", "failed"].map((value) => (
            <button key={value} className={filter === value ? "active" : ""} onClick={() => setFilter(value)}>{statusLabel(value)}</button>
          ))}
        </div>
        <button className="button button--secondary" onClick={load}><RefreshCw size={14} />Refresh</button>
      </div>
      {error && <ErrorState message={error} onRetry={load} />}
      <div className="runTableWrap">
        <table className="runTable">
          <thead><tr><th>Status</th><th>Experiment</th><th>Manifest</th><th>Phase</th><th>Compute</th><th>Queued</th><th aria-label="Actions" /></tr></thead>
          <tbody>
            {filtered.map((run) => {
              const Icon = statusIcons[run.status] || Activity;
              return (
                <tr key={run.id}>
                  <td><span className={`tableStatus tableStatus--${run.status}`}><Icon size={14} className={run.status === "running" ? "spin" : ""} />{statusLabel(run.status)}</span></td>
                  <td><strong>{run.name}</strong><small>{statusLabel(run.capability_id)}</small></td>
                  <td><span>Revision {run.manifest_revision}</span><small title={run.manifest_id}>{run.manifest_id.slice(0, 8)}</small></td>
                  <td><span>{run.phase}</span>{["running", "queued"].includes(run.status) && <div className="tableProgress"><i style={{ width: `${(run.progress || 0) * 100}%` }} /></div>}</td>
                  <td><span>{run.backend_used || run.compute?.preference || "auto"}</span><small>{run.compute?.candidate_count?.toLocaleString?.() || "—"} candidates</small></td>
                  <td>{formatDate(run.queued_at)}</td>
                  <td><div className="tableActions">{["running", "queued"].includes(run.status) && <button className="iconButton" title="Cancel" onClick={() => cancel(run.id)}><Ban size={15} /></button>}<Link href={`/?run=${run.id}`} className="iconButton" title="Open in laboratory"><ExternalLink size={15} /></Link></div></td>
                </tr>
              );
            })}
          </tbody>
        </table>
        {!filtered.length && <div className="tableEmpty">No runs match the current filter.</div>}
      </div>
    </section>
  );
}
