import { createContext, useContext, type ReactNode } from "react";

import type { NativeProtectionStatus } from "../types";
import type { ScanController } from "./useScanAction";

const NativeProtectionScanContext =
  createContext<ScanController<NativeProtectionStatus> | null>(null);

interface NativeProtectionScanProviderProps {
  controller: ScanController<NativeProtectionStatus>;
  children: ReactNode;
}

/**
 * Shares the user-triggered native-protection snapshot across pages. The provider
 * does not scan automatically; callers explicitly invoke the controller.
 */
export function NativeProtectionScanProvider({
  controller,
  children,
}: NativeProtectionScanProviderProps) {
  return (
    <NativeProtectionScanContext.Provider value={controller}>
      {children}
    </NativeProtectionScanContext.Provider>
  );
}

export function useNativeProtectionScan(): ScanController<NativeProtectionStatus> {
  const controller = useContext(NativeProtectionScanContext);
  if (controller === null) {
    throw new Error("useNativeProtectionScan must be used inside NativeProtectionScanProvider");
  }
  return controller;
}
