import { useEffect, useRef, useState } from "react";

import type { SensorStatusReport } from "../types";
import { getUserFacingApiError, type SentinelApi } from "./api";

export const SENSOR_STATUS_REFRESH_MS = 4_000;

export interface SensorStatusQueryState {
  phase: "loading" | "ready" | "error";
  report: SensorStatusReport | null;
  error: string | null;
  lastUpdatedAt: string | null;
}

export interface SensorStatusQuery {
  state: SensorStatusQueryState;
  refresh: () => Promise<void>;
}

const INITIAL_STATE: SensorStatusQueryState = {
  phase: "loading",
  report: null,
  error: null,
  lastUpdatedAt: null,
};

/**
 * Polls the read-only sensor status command while the home page is mounted.
 *
 * A failed query clears the previous report instead of leaving stale "healthy" data on screen.
 * Requests never overlap; the effect cleanup cancels any response that arrives after unmount or
 * after the API identity changes.
 */
export function useSensorStatus(api: SentinelApi): SensorStatusQuery {
  const [state, setState] = useState<SensorStatusQueryState>(INITIAL_STATE);
  const refreshRef = useRef<() => Promise<void>>(async () => undefined);

  useEffect(() => {
    let cancelled = false;
    let inFlight = false;
    let requestId = 0;

    async function run(): Promise<void> {
      if (cancelled || inFlight) {
        return;
      }

      inFlight = true;
      const currentRequest = requestId + 1;
      requestId = currentRequest;

      try {
        const report = await api.getSensorStatus();
        if (!cancelled && requestId === currentRequest) {
          setState({
            phase: "ready",
            report,
            error: null,
            lastUpdatedAt: new Date().toISOString(),
          });
        }
      } catch (error) {
        if (!cancelled && requestId === currentRequest) {
          setState({
            phase: "error",
            report: null,
            error: getUserFacingApiError(error),
            lastUpdatedAt: new Date().toISOString(),
          });
        }
      } finally {
        inFlight = false;
      }
    }

    refreshRef.current = run;
    void run();
    const timer = window.setInterval(() => {
      void run();
    }, SENSOR_STATUS_REFRESH_MS);

    return () => {
      cancelled = true;
      requestId += 1;
      window.clearInterval(timer);
      refreshRef.current = async () => undefined;
    };
  }, [api]);

  function refresh(): Promise<void> {
    return refreshRef.current();
  }

  return { state, refresh };
}
