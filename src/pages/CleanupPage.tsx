import { useSentinelApi } from "../components/ApiProvider";
import { ExpandableSection } from "../components/ExpandableSection";
import { ProcessPanel } from "../components/ProcessPanel";
import { SecurityPosture } from "../components/SecurityPosture";
import { ServiceTaskPanel } from "../components/ServiceTaskPanel";
import { SoftwarePanel } from "../components/SoftwarePanel";
import { StartupPanel } from "../components/StartupPanel";
import { useNativeProtectionScan } from "../lib/nativeProtectionScan";
import { useScanAction } from "../lib/useScanAction";

export function CleanupPage() {
  const api = useSentinelApi();
  const association = useScanAction(api.scanAssetAssociations);
  const protection = useNativeProtectionScan();

  return (
    <div className="page cleanup-page">
      <header className="cleanup-simple-header">
        <h1>软件体检</h1>
      </header>

      <SecurityPosture association={association} protection={protection} />

      <div className="cleanup-sections">
        <ExpandableSection title="软件">
          <div className="cleanup-panel">
            <SoftwarePanel association={association} />
          </div>
        </ExpandableSection>

        <ExpandableSection title="启动项">
          <div className="cleanup-panel">
            <StartupPanel />
          </div>
        </ExpandableSection>

        <ExpandableSection title="服务">
          <div className="cleanup-panel">
            <ServiceTaskPanel section="services" />
          </div>
        </ExpandableSection>

        <ExpandableSection title="计划任务">
          <div className="cleanup-panel">
            <ServiceTaskPanel section="tasks" />
          </div>
        </ExpandableSection>

        <ExpandableSection title="进程">
          <div className="cleanup-panel">
            <ProcessPanel />
          </div>
        </ExpandableSection>
      </div>
    </div>
  );
}
