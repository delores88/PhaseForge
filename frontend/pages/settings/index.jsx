import { useCallback, useState } from "react";
import AppShell from "@/components/shared/AppShell";
import SettingsView from "@/components/settings/SettingsView";
import { useBackendStatus } from "@/lib/useBackend";

export default function SettingsPage() {
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
      title="Settings"
      subtitle="AI providers, secret storage, and the agent execution boundary."
    >
      <SettingsView
        backend={backend}
        onHardware={handleHardware}
        onEventState={handleEventState}
      />
    </AppShell>
  );
}
