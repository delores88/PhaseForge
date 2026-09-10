import { useCallback, useEffect, useRef, useState } from "react";
import { api, eventsUrl } from "./api";

export function useBackendStatus() {
  const [status, setStatus] = useState({
    connected: false,
    checking: true,
    health: null,
    error: null,
  });

  const check = useCallback(async () => {
    try {
      const health = await api.health();
      setStatus({ connected: true, checking: false, health, error: null });
    } catch (error) {
      setStatus({
        connected: false,
        checking: false,
        health: null,
        error: error.message,
      });
    }
  }, []);

  useEffect(() => {
    check();
    const interval = window.setInterval(check, 8000);
    return () => window.clearInterval(interval);
  }, [check]);

  return { ...status, refresh: check };
}

export function useServerEvents(onEvent) {
  const handlerRef = useRef(onEvent);
  const [state, setState] = useState("disconnected");

  useEffect(() => {
    handlerRef.current = onEvent;
  }, [onEvent]);

  useEffect(() => {
    let socket;
    let retry;
    let closed = false;

    const connect = () => {
      if (closed) return;
      setState("connecting");
      socket = new WebSocket(eventsUrl());
      socket.onopen = () => setState("connected");
      socket.onmessage = (message) => {
        try {
          handlerRef.current?.(JSON.parse(message.data));
        } catch {
          // Ignore malformed event payloads; HTTP polling remains authoritative.
        }
      };
      socket.onerror = () => socket.close();
      socket.onclose = () => {
        setState("disconnected");
        if (!closed) retry = window.setTimeout(connect, 1800);
      };
    };

    connect();
    return () => {
      closed = true;
      window.clearTimeout(retry);
      socket?.close();
    };
  }, []);

  return state;
}
