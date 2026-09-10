import { useCallback, useState } from "react";
import AppShell from "@/components/shared/AppShell";
import ResearchWorkbench from "@/components/workspace/ResearchWorkbench";
import { useBackendStatus } from "@/lib/useBackend";

export default function LaboratoryPage() {
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
      title="Workspace / Laboratory"
    >
      <ResearchWorkbench
        backend={backend}
        hardware={hardware}
        onHardware={handleHardware}
        onEventState={handleEventState}
      />
    </AppShell>
  );
}
