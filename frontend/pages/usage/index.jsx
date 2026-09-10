import AppShell from "@/components/shared/AppShell";
import UsageView from "@/components/usage/UsageView";
import { useBackendStatus } from "@/lib/useBackend";
export default function UsagePage(){
  const backend=useBackendStatus();
  return <AppShell backend={backend} title="Usage & cost" subtitle="Tokens, local cost estimates, budgets, and live cancellation." eventState="http"><UsageView backend={backend}/></AppShell>;
}
