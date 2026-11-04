import { useEffect, useRef, useState } from "react";

import { getUserFacingApiError } from "./api";

export type ScanState<T> =
  | { phase: "idle" }
  | { phase: "loading" }
  | { phase: "error"; message: string }
  | { phase: "ready"; data: T };

export interface ScanController<T> {
  state: ScanState<T>;
  start: () => Promise<void>;
  reset: () => void;
}

export function useScanAction<T>(runScan: () => Promise<T>): ScanController<T> {
  const [state, setState] = useState<ScanState<T>>({ phase: "idle" });
  const requestSequence = useRef(0);
  const mounted = useRef(true);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      requestSequence.current += 1;
    };
  }, []);

  async function start(): Promise<void> {
    const requestId = requestSequence.current + 1;
    requestSequence.current = requestId;
    setState({ phase: "loading" });

    try {
      const data = await runScan();
      if (mounted.current && requestSequence.current === requestId) {
        setState({ phase: "ready", data });
      }
    } catch (error) {
      if (mounted.current && requestSequence.current === requestId) {
        setState({ phase: "error", message: getUserFacingApiError(error) });
      }
    }
  }

  function reset(): void {
    requestSequence.current += 1;
    setState({ phase: "idle" });
  }

  return { state, start, reset };
}
