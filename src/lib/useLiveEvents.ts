import { useEffect, useRef, useState } from "react";

import type { SensorLinkState } from "../types";
import { getUserFacingApiError, type SentinelApi } from "./api";
import {
  EMPTY_LIVE_STATE,
  mergeLiveBatch,
  type LiveEventState,
} from "./processEvents";

export const LIVE_EVENTS_REFRESH_MS = 1_000;
export const LIVE_EVENTS_ERROR_REFRESH_MS = 2_000;

export interface LiveEventsState {
  phase: "loading" | "ready" | "error";
  linkState: SensorLinkState | null;
  raw: LiveEventState;
  error: string | null;
  updatedAt: string | null;
}

export interface LiveEventsQuery {
  state: LiveEventsState;
  resume: () => void;
}

const INITIAL_STATE: LiveEventsState = {
  phase: "loading",
  linkState: null,
  raw: EMPTY_LIVE_STATE,
  error: null,
  updatedAt: null,
};

/**
 * Polls the read-only event endpoint while the event page is mounted.
 *
 * The cursor is advanced only after a batch is merged. A session change resets
 * the live list; cursor expiry or a sensor read failure is shown explicitly
 * instead of being rendered as an empty, healthy list. Requests never overlap.
 */
export function useLiveEvents(api: SentinelApi): LiveEventsQuery {
  const [state, setState] = useState<LiveEventsState>(INITIAL_STATE);
  const rawRef = useRef<LiveEventState>(EMPTY_LIVE_STATE);
  const resumeRef = useRef<() => void>(() => undefined);

  useEffect(() => {
    let cancelled = false;
    let timer: number | undefined;
    let inFlight = false;
    let stopped = false;
    let requestId = 0;

    function updateRaw(next: LiveEventState): void {
      rawRef.current = next;
      setState((current) => ({ ...current, raw: next }));
    }

    async function run(): Promise<void> {
      if (cancelled || inFlight || stopped) {
        return;
      }
      inFlight = true;
      const currentRequest = requestId + 1;
      requestId = currentRequest;
      let nextDelay = LIVE_EVENTS_REFRESH_MS;

      try {
        const report = await api.getSensorEvents(rawRef.current.nextAfter, 128);
        if (cancelled || requestId !== currentRequest) {
          return;
        }
        if (report.batch !== null) {
          const merged = mergeLiveBatch(rawRef.current, report.batch);
          updateRaw(merged);
          if (merged.sessionClosed) {
            stopped = true;
          }
        }
        setState((current) => ({
          ...current,
          phase: "ready",
          linkState: report.linkState,
          error: report.error,
          updatedAt: new Date().toISOString(),
        }));
        if (report.batch === null) {
          nextDelay = LIVE_EVENTS_ERROR_REFRESH_MS;
        }
      } catch (error) {
        if (cancelled || requestId !== currentRequest) {
          return;
        }
        setState((current) => ({
          ...current,
          phase: "error",
          error: getUserFacingApiError(error),
          updatedAt: new Date().toISOString(),
        }));
        nextDelay = LIVE_EVENTS_ERROR_REFRESH_MS;
      } finally {
        inFlight = false;
        if (!cancelled && !stopped) {
          timer = window.setTimeout(() => {
            void run();
          }, nextDelay);
        }
      }
    }

    resumeRef.current = () => {
      if (cancelled) {
        return;
      }
      stopped = false;
      requestId += 1;
      rawRef.current = EMPTY_LIVE_STATE;
      setState((current) => ({
        ...current,
        phase: "loading",
        linkState: null,
        raw: EMPTY_LIVE_STATE,
        error: null,
      }));
      if (timer !== undefined) {
        window.clearTimeout(timer);
      }
      void run();
    };

    void run();

    return () => {
      cancelled = true;
      requestId += 1;
      if (timer !== undefined) {
        window.clearTimeout(timer);
      }
      resumeRef.current = () => undefined;
    };
  }, [api]);

  return {
    state,
    resume: () => resumeRef.current(),
  };
}
