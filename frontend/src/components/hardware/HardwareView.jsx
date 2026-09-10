import {
  CheckCircle2,
  BrainCircuit,
  Cpu,
  Gauge,
  MemoryStick,
  MonitorUp,
  RefreshCw,
  TriangleAlert,
} from "lucide-react";
import { useCallback, useEffect, useState } from "react";

import { api } from "@/lib/api";
import { formatBytes } from "@/lib/format";
import { ErrorState, LoadingState } from "@/components/shared/AsyncState";

export default function HardwareView({ backend, onHardware, onEventState }) {
  const [hardware, setHardware] = useState(null);
  const [browserGpu, setBrowserGpu] = useState(null);
  const [error, setError] = useState(null);
  const [loading, setLoading] = useState(true);

  const load = useCallback(async () => {
    if (!backend.connected) return;
    try {
      setError(null);
      const value = await api.hardware();
      setHardware(value);
      onHardware?.(value);
    } catch (loadError) {
      setError(loadError.message);
    } finally {
      setLoading(false);
    }
  }, [backend.connected, onHardware]);

  useEffect(() => {
    load();
  }, [load]);

  useEffect(() => {
    onEventState?.(backend.connected ? "http" : "disconnected");
  }, [backend.connected, onEventState]);

  useEffect(() => {
    let cancelled = false;
    async function inspectBrowser() {
      const canvas = document.createElement("canvas");
      const gl = canvas.getContext("webgl2", { powerPreference: "high-performance" });
      let renderer = "WebGL2 unavailable";
      if (gl) {
        const extension = gl.getExtension("WEBGL_debug_renderer_info");
        renderer = extension
          ? gl.getParameter(extension.UNMASKED_RENDERER_WEBGL)
          : gl.getParameter(gl.RENDERER);
        gl.getExtension('WEBGL_lose_context')?.loseContext();
      }

      let webgpu = false;
      let adapterInfo = null;
      try {
        if (navigator.gpu) {
          const adapter = await navigator.gpu.requestAdapter({
            powerPreference: "high-performance",
          });
          webgpu = Boolean(adapter);
          if (adapter?.info) adapterInfo = { ...adapter.info };
        }
      } catch {
        // Browser rendering still falls back to WebGL2.
      }
      if (!cancelled) setBrowserGpu({ renderer, webgl2: Boolean(gl), webgpu, adapterInfo });
    }
    inspectBrowser();
    return () => {
      cancelled = true;
    };
  }, []);

  if (loading && backend.connected) {
    return <LoadingState label="Profiling local compute…" />;
  }

  if (error) {
    return <ErrorState message={error} onRetry={load} />;
  }

  if (!hardware) return null;

  const neural = hardware.neural_accelerators;
  const neuralDevices = Array.isArray(neural?.devices) ? neural.devices : [];
  const neuralProbeComplete = neural?.probe_status === "complete";

  return (
    <section className="hardwareView">
      <div className="hardwareSummary">
        <SummaryCard
          icon={Cpu}
          label="CPU"
          value={hardware.cpu?.brand}
          detail={`${hardware.cpu?.logical_cores || "—"} logical cores`}
        />
        <SummaryCard
          icon={MemoryStick}
          label="Memory"
          value={formatBytes(hardware.cpu?.total_memory_bytes)}
          detail={`${hardware.scheduler?.cpu_worker_threads || "—"} compute workers`}
        />
        <SummaryCard
          icon={Gauge}
          label="Scheduler"
          value="1 numerical run at a time"
          detail={`${hardware.scheduler?.max_parallel_jobs || "—"} dispatcher slots`}
        />
        <SummaryCard
          icon={MonitorUp}
          label="Browser renderer"
          value={browserGpu ? (browserGpu.webgl2 ? "WebGL2 available" : "WebGL2 unavailable") : "Checking graphics"}
          detail={browserGpu?.renderer || "Inspecting browser adapter"}
        />
      </div>

      <div className="hardwareColumns">
        <section className="surfacePanel">
          <header className="surfacePanel__header">
            <div>
              <h2>Compute adapters</h2>
              <p>
                Compatible ODE candidate scoring can use GPU compute. Final
                trajectories and particle simulations use CPU precision. Scene
                rendering uses the browser's graphics adapter independently.
              </p>
            </div>
            <button className="button button--secondary" onClick={load}>
              <RefreshCw size={14} />
              Refresh profile
            </button>
          </header>

          <div className="adapterList">
            {hardware.adapters?.map((adapter) => (
              <article
                key={`${adapter.index}-${adapter.name}-${adapter.backend}`}
                className={`adapterCard ${adapter.selected ? "adapterCard--selected" : ""}`}
              >
                <div className="adapterCard__icon">
                  {adapter.selected ? (
                    <CheckCircle2 size={20} />
                  ) : (
                    <MonitorUp size={20} />
                  )}
                </div>
                <div className="adapterCard__body">
                  <div>
                    <strong>{adapter.name}</strong>
                    {adapter.selected && <span>Compatible compute adapter</span>}
                  </div>
                  <dl>
                    <div><dt>Vendor</dt><dd>{adapter.vendor}</dd></div>
                    <div><dt>Backend</dt><dd>{adapter.backend}</dd></div>
                    <div><dt>Type</dt><dd>{adapter.device_type}</dd></div>
                    <div><dt>Driver</dt><dd>{adapter.driver || "—"}</dd></div>
                    <div>
                      <dt>Buffer binding limit</dt>
                      <dd>{formatBytes(adapter.max_storage_buffer_binding_size)}</dd>
                    </div>
                    <div><dt>Selection score</dt><dd>{adapter.score}</dd></div>
                  </dl>
                </div>
              </article>
            ))}
            {!hardware.adapters?.length && (
              <div className="tableEmpty">
                No wgpu adapter was returned. CPU fallback is active.
              </div>
            )}
          </div>
        </section>

        <section className="surfacePanel">
          <header className="surfacePanel__header">
            <div>
              <h2>Dispatch policy</h2>
              <p>Each experiment is sized against its approved limits and available host memory.</p>
            </div>
          </header>
          <ol className="policyList">
            <li>One numerical experiment runs at a time. Other research tasks can continue while simulation jobs queue.</li>
            <li>CPU batches respect the run's memory cap and reserve half of available host RAM at admission. Memory estimates are allocation guidance, not an operating-system memory limit.</li>
            <li>{hardware.gpu_available ? "Compatible ODE searches score candidates on GPU using 32-bit arithmetic; final replay and trajectory measurements use 64-bit CPU arithmetic." : "No GPU compute adapter was initialized. Numerical experiments use bounded CPU batches."}</li>
            <li>GPU batch sizes stay within adapter and approved allocation limits. Dispatch timing or allocation pressure can reduce the batch; the displayed reference is not a throughput measurement.</li>
          </ol>
          <div className="policyGrid">
            <div><span>Dispatcher slot limit</span><strong>{hardware.scheduler?.max_parallel_jobs}</strong></div>
            <div><span>CPU worker budget</span><strong>{hardware.scheduler?.cpu_worker_threads}</strong></div>
            <div><span>GPU batch reference</span><strong>{hardware.gpu_available ? hardware.scheduler?.gpu_batch_size?.toLocaleString?.() || "Not reported" : "Unavailable"}</strong></div>
            <div><span>Numerical run slots</span><strong>1</strong></div>
          </div>

          {hardware.warnings?.map((warning) => (
            <div className="warningNotice" key={warning}>
              <TriangleAlert size={16} />
              <span>{warning}</span>
            </div>
          ))}
        </section>
      </div>

      {neural && <section className="surfacePanel" aria-label="Neural accelerator inventory">
        <header className="surfacePanel__header">
          <div>
            <h2>Neural and AI accelerators</h2>
            <p>{neural.scope || "Operating-system device inventory. Detection alone does not establish numerical solver support."}</p>
          </div>
        </header>
        <div className="policyGrid">
          <div><span>Inventory status</span><strong>{neuralProbeComplete ? "Scan complete" : "Unavailable"}</strong></div>
          <div><span>Devices reported</span><strong>{neuralProbeComplete ? neuralDevices.length : "Unknown"}</strong></div>
          <div><span>NPU numerical execution</span><strong>{neural.numerical_execution_available === true ? "Reported available" : neural.numerical_execution_available === false ? "No solver installed" : "Not reported"}</strong></div>
          <div><span>Simulation allocation</span><strong>{neural.numerical_execution_available === true ? "Check solver eligibility" : "CPU / GPU only"}</strong></div>
        </div>
        <div className="adapterList">
          {neuralDevices.map((device,index)=><article className="adapterCard" key={`${device.Name || device.name || "accelerator"}-${index}`}>
            <div className="adapterCard__icon"><BrainCircuit size={20}/></div>
            <div className="adapterCard__body">
              <div><strong>{device.Name || device.name || "Unnamed accelerator"}</strong></div>
              <dl>
                <div><dt>Manufacturer</dt><dd>{device.Manufacturer || device.manufacturer || "Not reported"}</dd></div>
                <div><dt>OS status</dt><dd>{device.Status || device.status || "Not reported"}</dd></div>
                <div><dt>Driver</dt><dd>{device.driver || "Not reported"}</dd></div>
              </dl>
            </div>
          </article>)}
          {!neuralDevices.length&&<div className="tableEmpty">{neuralProbeComplete ? "No matching devices were returned by this operating-system inventory." : "This system did not provide a supported inventory. Accelerator presence and capacity are unknown."}</div>}
        </div>
      </section>}

      <section className="surfacePanel" aria-label="Numerical recovery behavior">
        <header className="surfacePanel__header"><div><h2>Recovery during a run</h2><p>Resource adaptation preserves the experiment's equations, candidate budget and original deadline.</p></div></header>
        <ol className="policyList">
          <li>GPU allocation pressure triggers up to four smaller-batch retries before CPU fallback. A 30-second readback watchdog disables further dispatches on a stalled GPU until the backend restarts.</li>
          <li>Completed search generations are checkpointed locally. Memory-pressure recovery uses smaller CPU batches and the last compatible checkpoint; partially evaluated generations and final integration replay restart.</li>
          <li>Standalone interrupted simulations can recover up to three times within their original deadline. Research tasks pause after an app restart; resume the owning task to continue.</li>
          <li>Numerical divergence and invalid equations remain failures to inspect. Recovery events are retained with the run's numerical evidence.</li>
        </ol>
      </section>
    </section>
  );
}

function SummaryCard({ icon: Icon, label, value, detail }) {
  return (
    <article className="summaryCard">
      <Icon size={18} />
      <div>
        <span>{label}</span>
        <strong title={value}>{value || "—"}</strong>
        <small title={detail}>{detail}</small>
      </div>
    </article>
  );
}
