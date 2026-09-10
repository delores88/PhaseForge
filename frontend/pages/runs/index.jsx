import { useCallback, useState } from "react";
import AppShell from "@/components/shared/AppShell";
import RunsView from "@/components/runs/RunsView";
import { useBackendStatus } from "@/lib/useBackend";

export default function RunsPage() {
  const backend = useBackendStatus();
  const [hardware, setHardware] = useState(null);
  const [eventState, setEventState] = useState("disconnected");
  const handleHardware = useCallback((value) => setHardware(value), []);
  const handleEventState = useCallback((value) => setEventState(value), []);

  return (
    <AppShell
      backend={backend}
      hardware={hardware}
      eventState={eventState}
      title="Experiment runs"
      subtitle="Queue state, execution history, and reproducible outputs."
    >
      <RunsView
        backend={backend}
        onHardware={handleHardware}
        onEventState={handleEventState}
      />
    </AppShell>
  );
}
