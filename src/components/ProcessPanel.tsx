import { useEffect, useId, useRef, useState } from "react";

import { useSentinelApi } from "./ApiProvider";
import { DataNotes } from "./DataNotes";
import { FileEvidenceView } from "./FileEvidenceView";
import { InventoryIncompleteNotice } from "./InventoryIncompleteNotice";
import { ListPagination, paginateItems } from "./ListPagination";
import { RiskBadge } from "./RiskBadge";
import { RiskEvidence } from "./RiskEvidence";
import { getUserFacingApiError } from "../lib/api";
import { formatCheckedAt, formatCpuIndices } from "../lib/presentation";
import {
  inspectionStateLabel,
  processPathAccessLabel,
  signatureStateLabel,
} from "../lib/uiLabels";
import { useScanAction } from "../lib/useScanAction";
import type { AffinityInspection } from "../types";

type InspectionState =
  | { phase: "idle" }
  | { phase: "loading"; pid: number }
  | { phase: "error"; pid: number; message: string }
  | { phase: "ready"; data: AffinityInspection };

export function ProcessPanel() {
  const api = useSentinelApi();
  const { state, start } = useScanAction(api.listProcesses);
  const [query, setQuery] = useState("");
  const [page, setPage] = useState(1);
  const [inspection, setInspection] = useState<InspectionState>({ phase: "idle" });
  const inspectionSequence = useRef(0);
  const searchId = useId();

  useEffect(
    () => () => {
      inspectionSequence.current += 1;
    },
    [],
  );

  function runScan(): void {
    inspectionSequence.current += 1;
    setInspection({ phase: "idle" });
    setPage(1);
    void start();
  }

  async function inspectProcess(pid: number, identity: string): Promise<void> {
    const sequence = inspectionSequence.current + 1;
    inspectionSequence.current = sequence;
    setInspection({ phase: "loading", pid });

    try {
      const data = await api.inspectProcessAffinity(pid, identity);
      if (inspectionSequence.current === sequence) {
        setInspection({ phase: "ready", data });
      }
    } catch (error) {
      if (inspectionSequence.current === sequence) {
        setInspection({
          phase: "error",
          pid,
          message: getUserFacingApiError(error),
        });
      }
    }
  }

  const data = state.phase === "ready" ? state.data : null;
  const sortedProcesses = data
    ? [...data.processes].sort((left, right) =>
        left.name.localeCompare(right.name, "zh-CN"),
      )
    : [];

  const normalizedQuery = query.trim().toLowerCase();
  const filteredProcesses = sortedProcesses.filter((process) => {
    if (normalizedQuery.length === 0) {
      return true;
    }
    return [
      process.name,
      String(process.pid),
      process.executablePath,
      process.parentName,
    ].some((value) => value?.toLowerCase().includes(normalizedQuery));
  });
  const {
    items: visibleProcesses,
    page: currentPage,
    pageCount,
  } = paginateItems(filteredProcesses, page);

  const scanLabel =
    state.phase === "loading"
      ? "正在读取…"
      : state.phase === "error"
        ? "重新尝试"
        : "刷新进程快照";

  return (
    <section className="scan-panel" aria-labelledby="process-panel-heading">
      <header className="scan-panel__header">
        <div>
          <h2 id="process-panel-heading">进程与 CPU 亲和性</h2>
          <p>
            读取当前进程快照；选择进程后进行只读的亲和性、签名和短时 CPU 时间检查。
          </p>
        </div>
        <button
          type="button"
          className="button button--primary"
          onClick={runScan}
          disabled={state.phase === "loading"}
        >
          {scanLabel}
        </button>
      </header>

      {state.phase === "error" ? (
        <div className="notice notice--error" role="alert">
          <div>
            <h3>进程快照未完成</h3>
            <p>{state.message}</p>
          </div>
        </div>
      ) : null}

      {data ? (
        <>
          {data.truncated ? (
            <InventoryIncompleteNotice scope="进程快照" issues={data.issues} />
          ) : null}

          <div className="scan-summary">
            <div>
              <span className="scan-summary__value">{data.processes.length}</span>
              <span>{data.truncated ? "个已收集进程（不完整）" : "个可见进程"}</span>
            </div>
            <p>快照时间：{formatCheckedAt(data.checkedAt)}</p>
            <p>这是某一时刻的进程快照；进程可能在读取后退出或 PID 被复用。</p>
          </div>

          <div className="caveat-box">
            <strong>先取证据，再解释原因</strong>
            <p>
              处理器亲和性受限本身不等同于恶意行为。高 CPU 占用也不证明亲和性被修改。
              检查使用创建时间身份令牌，不能把旧 PID 名称与新进程数据混合。
            </p>
          </div>

          <div className="list-toolbar">
            <label className="search-field" htmlFor={searchId}>
              <span>筛选</span>
              <input
                id={searchId}
                type="search"
                value={query}
                onChange={(event) => {
                  setQuery(event.target.value);
                  setPage(1);
                }}
                placeholder="进程名、PID、路径或父进程"
                autoComplete="off"
              />
            </label>
            <span className="list-count">
              显示 {visibleProcesses.length} / {filteredProcesses.length}
            </span>
          </div>

          <div className="workspace-grid">
            <div className="table-column">
              {visibleProcesses.length > 0 ? (
                <div
                  className="table-scroll"
                  role="region"
                  aria-label="当前进程快照（可横向滚动）"
                  tabIndex={0}
                >
                  <table className="data-table" aria-label="当前进程快照">
                    <thead>
                      <tr>
                        <th scope="col">进程 / 父进程</th>
                        <th scope="col" className="col-action">
                          只读检查
                        </th>
                      </tr>
                    </thead>
                    <tbody>
                      {visibleProcesses.map((process) => (
                        <tr
                          key={process.pid}
                          className={
                            inspection.phase === "ready" && inspection.data.pid === process.pid
                              ? "is-selected"
                              : undefined
                          }
                        >
                          <td>
                            <span className="row-title">{process.name}</span>
                            <span className="cell-note mono-text">PID {process.pid}</span>
                            <span className="cell-note">
                              {process.parentName ?? "父进程未知"} · PID {process.parentPid} ·{" "}
                              {processPathAccessLabel(process.pathAccess)}
                            </span>
                            {process.identity === null ? (
                              <span className="cell-note">
                                进程身份时间不可读，无法安全检查
                              </span>
                            ) : null}
                          </td>
                          <td className="col-action">
                            <button
                              type="button"
                              className="button button--compact"
                              aria-label={`检查 PID ${process.pid} 的亲和性与签名`}
                              disabled={
                                process.identity === null || inspection.phase === "loading"
                              }
                              onClick={() => {
                                if (process.identity !== null) {
                                  void inspectProcess(process.pid, process.identity);
                                }
                              }}
                            >
                              检查
                            </button>
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              ) : (
                <div className="empty-block">
                  <p>当前筛选没有匹配项。</p>
                  <p>进程列表可能缺少受保护或权限不足的进程路径信息。</p>
                </div>
              )}

              <ListPagination
                page={currentPage}
                pageCount={pageCount}
                onPageChange={setPage}
                label="进程列表分页"
              />
            </div>

            <aside className="detail-column" aria-live="polite">
              <ProcessDetail inspection={inspection} />
            </aside>
          </div>

          <DataNotes limitations={data.limitations} issues={data.issues} />
        </>
      ) : null}
    </section>
  );
}

function ProcessDetail({ inspection }: { inspection: InspectionState }) {
  if (inspection.phase === "idle") {
    return (
      <div className="detail-placeholder">
        <h3>亲和性证据面板</h3>
        <p>选择一个进程执行只读检查。不会结束进程或修改处理器设置。</p>
      </div>
    );
  }

  if (inspection.phase === "loading") {
    return (
      <div className="detail-placeholder" role="status">
        <h3>正在读取 PID {inspection.pid}</h3>
        <p>正在核验创建时间身份、采样约 500 毫秒的 CPU 时间，并验证磁盘文件签名。</p>
      </div>
    );
  }

  if (inspection.phase === "error") {
    return (
      <div className="detail-card">
        <p className="detail-overline">PID {inspection.pid}</p>
        <h3>无法完成亲和性检查</h3>
        <p>{inspection.message}</p>
        <p className="fine-print">
          权限不足、进程已退出或身份变化时，结果保持未知，不推断为安全或恶意。
        </p>
      </div>
    );
  }

  const data = inspection.data;

  return (
    <div className="detail-card">
      <header className="detail-card__header">
        <div>
          <p className="detail-overline">
            PID {data.pid} · {inspectionStateLabel(data.inspectionState)}
          </p>
          <h3>{data.name}</h3>
          <p>{data.executablePath}</p>
        </div>
        <div className="scope-verdict" aria-label="整体进程结论">
          <span>整体进程结论</span>
          <RiskBadge level={data.overallRisk.level} />
        </div>
      </header>

      <dl className="fact-list">
        <div>
          <dt>进程身份</dt>
          <dd>已绑定创建时间（不依赖 PID 单独识别）</dd>
        </div>
        <div>
          <dt>启动时间</dt>
          <dd>{data.startTime ? formatCheckedAt(data.startTime) : "未读取"}</dd>
        </div>
        <div>
          <dt>检查时间</dt>
          <dd>{formatCheckedAt(data.checkedAt)}</dd>
        </div>
      </dl>

      <section className="inspection-block" aria-labelledby="affinity-heading">
        <div className="inspection-block__heading">
          <h4 id="affinity-heading">处理器亲和性</h4>
          {data.affinity ? <span className="mono-note">当前处理器组</span> : null}
        </div>

        <div className="scope-verdict" aria-label="亲和性配置结论">
          <span>亲和性配置结论</span>
          <RiskBadge level={data.affinityRisk.level} />
          <p>{data.affinityRisk.summary}</p>
        </div>

        {data.affinity ? (
          <>
            <p className="affinity-verdict">
              {data.affinity.restricted ? "处理器范围受限" : "处理器范围与系统一致"}
            </p>
            <div className="affinity-grid">
              <div>
                <span>系统可用</span>
                <strong>{formatCpuIndices(data.affinity.systemMask.cpuIndices)}</strong>
                <small>{data.affinity.systemMask.count} 个逻辑处理器</small>
              </div>
              <div>
                <span>该进程允许</span>
                <strong>{formatCpuIndices(data.affinity.processMask.cpuIndices)}</strong>
                <small>{data.affinity.processMask.count} 个逻辑处理器</small>
              </div>
            </div>
            <p className="fine-print">
              掩码：进程 {data.affinity.processMask.maskHex} / 系统{" "}
              {data.affinity.systemMask.maskHex}
            </p>
            <p className="affinity-caveat">
              处理器亲和性受限不等同于恶意行为。某些软件会主动限制线程范围，也可能来自用户或其他管理策略。
            </p>
            <div className="source-trace">
              <span>设置来源是否可观察</span>
              <strong>{data.affinity.sourceTrace.available ? "可观察" : "不可观察"}</strong>
              <p>{data.affinity.sourceTrace.explanation}</p>
            </div>
          </>
        ) : (
          <p>无法读取处理器亲和性。该结果不能证明进程安全，也不能证明它被限制。</p>
        )}
      </section>

      <section className="inspection-block" aria-labelledby="cpu-heading">
        <div className="inspection-block__heading">
          <h4 id="cpu-heading">短时 CPU 采样</h4>
          {data.cpuUsage ? (
            <span className="mono-note">{data.cpuUsage.sampleDurationMs} ms</span>
          ) : null}
        </div>
        {data.cpuUsage ? (
          <>
            <p className="usage-value">
              <strong>{data.cpuUsage.percentOfTotalCapacity.toFixed(1)}%</strong>
              <span>占当前处理器组容量</span>
            </p>
            <p className="fine-print">{data.cpuUsage.method}</p>
            <p className="fine-print">{data.cpuUsage.caveat}</p>
          </>
        ) : (
          <p>未获得 CPU 时间样本。</p>
        )}
      </section>

      <section className="inspection-block" aria-labelledby="signature-heading">
        <div className="inspection-block__heading">
          <h4 id="signature-heading">文件签名与磁盘证据</h4>
          <span className="state-label">{signatureStateLabel(data.signature.state)}</span>
        </div>
        <p>{data.signature.detail}</p>
        <dl className="fact-list fact-list--compact">
          <div>
            <dt>签名者</dt>
            <dd>{data.signature.signerName ?? "未提供或无法读取"}</dd>
          </div>
          <div>
            <dt>网络与吊销</dt>
            <dd>未联网；未检查证书吊销</dd>
          </div>
          <div>
            <dt>内存映像</dt>
            <dd>未验证；磁盘证据不能证明运行内存未变化</dd>
          </div>
        </dl>
        <FileEvidenceView evidence={data.signature.fileEvidence} />
      </section>

      <RiskEvidence risk={data.overallRisk} />

      <p className="read-only-note">
        第一阶段只读：不会修改处理器亲和性、结束进程或更改系统配置。
      </p>

      <DataNotes limitations={data.limitations} issues={data.issues} />
    </div>
  );
}
