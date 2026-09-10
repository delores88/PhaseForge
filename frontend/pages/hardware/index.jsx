import ResourceMonitor from "@/components/workspace/ResourceMonitor";
import { useCallback, useState } from "react";
import AppShell from "@/components/shared/AppShell";
import HardwareView from "@/components/hardware/HardwareView";
import { useBackendStatus } from "@/lib/useBackend";

export default function HardwarePage() {
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
      title="Compute"
      subtitle="Native adapters, browser rendering, and the active dispatch policy."
    >
      <ResourceMonitor full />
      <HardwareView
        backend={backend}
        onHardware={handleHardware}
        onEventState={handleEventState}
      />
    </AppShell>
  );
}
