import { AlertTriangle, LoaderCircle, RefreshCw, WifiOff } from "lucide-react";

export function LoadingState({ label = "Loading…" }) {
  return (
    <div className="asyncState">
      <LoaderCircle className="spin" size={22} />
      <span>{label}</span>
    </div>
  );
}

export function ErrorState({ title = "Unable to load", message, onRetry }) {
  return (
    <div className="asyncState asyncState--error">
      <AlertTriangle size={22} />
      <div>
        <strong>{title}</strong>
        <span>{message || "The request failed."}</span>
      </div>
      {onRetry && (
        <button className="button button--secondary" onClick={onRetry}>
          <RefreshCw size={14} />
          Retry
        </button>
      )}
    </div>
  );
}

export function OfflineState() {
  return (
    <div className="offlineBanner">
      <WifiOff size={16} />
      <span>
        Backend is offline. Start <code>START_BACKEND.txt</code>, then refresh.
      </span>
    </div>
  );
}
