import { createContext, useContext, type ReactNode } from "react";

import { tauriApi, type SentinelApi } from "../lib/api";

const SentinelApiContext = createContext<SentinelApi | null>(null);

interface ApiProviderProps {
  api?: SentinelApi;
  children: ReactNode;
}

export function ApiProvider({ api = tauriApi, children }: ApiProviderProps) {
  return <SentinelApiContext.Provider value={api}>{children}</SentinelApiContext.Provider>;
}

export function useSentinelApi(): SentinelApi {
  const api = useContext(SentinelApiContext);
  if (api === null) {
    throw new Error("useSentinelApi must be used inside ApiProvider");
  }
  return api;
}
