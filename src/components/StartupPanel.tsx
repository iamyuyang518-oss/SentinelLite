import { useId, useState } from "react";

import { useSentinelApi } from "./ApiProvider";
import { DataNotes } from "./DataNotes";
import { InventoryIncompleteNotice } from "./InventoryIncompleteNotice";
import { IssueNote } from "./IssueNote";
import { ListPagination, paginateItems } from "./ListPagination";
import { RiskBadge } from "./RiskBadge";
import { formatCheckedAt, startupSourceLabel } from "../lib/presentation";
import { redactUserPath } from "../lib/privacy";
import {
  installScopeLabel,
  inventoryObservationLabel,
  pathStateLabel,
  registryViewLabel,
  startupPathResolutionLabel,
  startupTargetKindLabel,
  startupValueTypeLabel,
} from "../lib/uiLabels";
import { useScanAction } from "../lib/useScanAction";
import type {
  InventoryObservation,
  StartupItem,
  StartupSourceStatus,
} from "../types";

function ObservationBadge({ observation }: { observation: InventoryObservation }) {
  return (
    <span className={`inventory-badge inventory-badge--${observation}`}>
      {inventoryObservationLabel(observation)}
    </span>
  );
}

function sourceStatusLabel(source: StartupSourceStatus): string {
  if (source.exists === false) {
    return "不存在";
  }
  return inventoryObservationLabel(source.observation);
}

function sourceStatusClass(source: StartupSourceStatus): string {
  return source.exists === false ? "notPresent" : source.observation;
}

function presenceLabel(value: boolean | null, present: string): string {
  if (value === null) {
    return "未知（未读取）";
  }
  return value ? present : "未提供";
}

export function StartupPanel() {
  const api = useSentinelApi();
  const { state, start } = useScanAction(api.scanStartupItems);
  const [query, setQuery] = useState("");
  const [page, setPage] = useState(1);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const searchId = useId();

  function runScan(): void {
    setSelectedId(null);
    setPage(1);
    void start();
  }

  const data = state.phase === "ready" ? state.data : null;
  const sortedItems = data
    ? [...data.items].sort((left, right) => left.name.localeCompare(right.name, "zh-CN"))
    : [];

  const normalizedQuery = query.trim().toLowerCase();
  const filteredItems = sortedItems.filter((item) => {
    if (normalizedQuery.length === 0) {
      return true;
    }
    return [
      item.name,
      item.valueName,
      item.targetPath,
      item.targetCandidate,
      startupSourceLabel(item.source),
      installScopeLabel(item.scope),
    ].some((value) => value?.toLowerCase().includes(normalizedQuery));
  });
  const {
    items: visibleItems,
    page: currentPage,
    pageCount,
  } = paginateItems(filteredItems, page);

  const selected = data?.items.find((item) => item.id === selectedId) ?? null;

  const scanLabel =
    state.phase === "loading"
      ? "正在读取…"
      : state.phase === "ready"
        ? "重新盘点"
        : state.phase === "error"
          ? "重新尝试"
          : "开始启动项检查";

  return (
    <section className="scan-panel" aria-labelledby="startup-panel-heading">
      <header className="scan-panel__header">
        <div>
          <h2 id="startup-panel-heading">启动项只读盘点</h2>
          <p>
            读取当前用户/本机的 Run、RunOnce 键和用户/所有用户启动文件夹。只读观察，
            不修改、禁用、删除或执行启动项。
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

      {state.phase === "loading" ? (
        <p className="inventory-idle" role="status">
          正在读取 Run、RunOnce 和启动文件夹；不会修改、禁用或执行启动项。
        </p>
      ) : null}

      {state.phase === "error" ? (
        <div className="notice notice--error" role="alert">
          <div>
            <h3>启动项盘点未完成</h3>
            <p>{state.message}</p>
            <p className="fine-print">
              未保留上一次结果；空列表或读取失败都不表示系统没有启动项。
            </p>
          </div>
        </div>
      ) : null}

      {data ? (
        <>
          {data.truncated ? (
            <InventoryIncompleteNotice scope="启动项盘点" issues={data.issues} />
          ) : null}

          <div className="scan-summary">
            <div>
              <span className="scan-summary__value">{data.items.length}</span>
              <span>{data.truncated ? "个已收集启动项（不完整）" : "个启动项"}</span>
            </div>
            <ObservationBadge observation={data.observation} />
            <p>读取时间：{formatCheckedAt(data.checkedAt)}</p>
            <p>
              {data.observation === "observed"
                ? "本次覆盖的位置未标记读取失败；这不代表启动项会运行、已启用或安全。"
                : "存在部分未知或读取失败；请查看来源状态、条目状态和读取问题。"}
            </p>
          </div>

          <div className="caveat-box caveat-box--attention">
            <strong>启用状态未核验</strong>
            <p>
              Run 键或启动文件夹中出现条目，不代表 Windows 会实际执行，也不代表用户知情。
              启动审批结果、计划任务、服务与策略启动项没有在本阶段核验；本页不显示“已启用”或“正常”的确定性结论。
            </p>
          </div>

          <section className="startup-sources" aria-labelledby="startup-sources-heading">
            <div className="startup-sources__header">
              <h3 id="startup-sources-heading">数据来源</h3>
              <p>每个来源独立读取；一个来源失败不会清空其他来源。</p>
            </div>
            <div className="startup-source-grid">
              {data.sources.map((source) => (
                <article className="startup-source-card" key={source.id}>
                  <header>
                    <strong>{startupSourceLabel(source.source)}</strong>
                    <span
                      className={`inventory-badge inventory-badge--${sourceStatusClass(source)}`}
                    >
                      {sourceStatusLabel(source)}
                    </span>
                  </header>
                  <p>
                    {installScopeLabel(source.scope)}
                    {source.registryView
                      ? ` · ${registryViewLabel(source.registryView)}`
                      : ""}
                  </p>
                  <p className="mono-text breakable">{source.location}</p>
                  <p className="fine-print">{source.itemCount} 个条目</p>
                </article>
              ))}
            </div>
          </section>

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
                placeholder="名称、目标路径或来源"
                autoComplete="off"
              />
            </label>
            <span className="list-count">
              显示 {visibleItems.length} / {filteredItems.length}
            </span>
          </div>

          <div className="workspace-grid">
            <div className="table-column">
              {visibleItems.length > 0 ? (
                <div
                  className="table-scroll"
                  role="region"
                  aria-label="启动项列表（可横向滚动）"
                  tabIndex={0}
                >
                  <table className="data-table inventory-table startup-table" aria-label="启动项列表">
                    <thead>
                      <tr>
                        <th scope="col">启动项</th>
                        <th scope="col">来源 / 用户范围</th>
                        <th scope="col">关联目标</th>
                        <th scope="col">观察</th>
                      </tr>
                    </thead>
                    <tbody>
                      {visibleItems.map((item) => (
                        <tr
                          key={item.id}
                          className={item.id === selectedId ? "is-selected" : undefined}
                        >
                          <td>
                            <button
                              type="button"
                              className="row-link"
                              aria-label={`查看 ${item.name} 的详情`}
                              aria-expanded={item.id === selectedId}
                              aria-controls="startup-detail-panel"
                              onClick={() => setSelectedId(item.id)}
                            >
                              {item.name}
                            </button>
                            <span className="cell-note">
                              {startupTargetKindLabel(item.targetKind)}
                              {item.valueType
                                ? ` · ${startupValueTypeLabel(item.valueType)}`
                                : ""}
                            </span>
                            <IssueNote issues={item.issues} />
                          </td>
                          <td>
                            {startupSourceLabel(item.source)}
                            <span className="cell-note">
                              {installScopeLabel(item.scope)}
                              {item.registryView
                                ? ` · ${registryViewLabel(item.registryView)}`
                                : ""}
                            </span>
                          </td>
                          <td>
                            <span className="mono-text breakable">
                              {redactUserPath(item.targetPath) ?? "目标未解析"}
                            </span>
                            <span className="cell-note">{pathStateLabel(item.targetState)}</span>
                            {item.targetKind === "shortcut" ? (
                              <span className="cell-note">
                                参数：{presenceLabel(item.shortcutArgumentsPresent, "存在")} · 工作目录：
                                {presenceLabel(item.shortcutWorkingDirectoryPresent, "存在")}
                              </span>
                            ) : null}
                          </td>
                          <td>
                            <ObservationBadge observation={item.observation} />
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              ) : (
                <div className="empty-block">
                  <p>
                    {data.items.length === 0
                      ? "已覆盖的位置没有返回可显示启动项。"
                      : "当前筛选没有匹配项。"}
                  </p>
                  <p>覆盖范围有限，空列表或不匹配不能证明系统没有其他持久化机制。</p>
                </div>
              )}

              <ListPagination
                page={currentPage}
                pageCount={pageCount}
                onPageChange={setPage}
                label="启动项分页"
              />
            </div>

            <aside
              id="startup-detail-panel"
              className="detail-column"
              role="region"
              aria-label="启动项详情"
              aria-live="polite"
            >
              {selected ? (
                <StartupDetail item={selected} />
              ) : (
                <div className="detail-placeholder">
                  <h3>证据面板</h3>
                  <p>选择一项启动项，查看来源、值类型、关联目标、读取状态与盲区。</p>
                </div>
              )}
            </aside>
          </div>

          <DataNotes limitations={data.limitations} issues={data.issues} />
        </>
      ) : null}
    </section>
  );
}

function StartupDetail({ item }: { item: StartupItem }) {
  return (
    <div className="detail-card">
      <header className="detail-card__header">
        <div>
          <p className="detail-overline">启动项</p>
          <h3>{item.name}</h3>
          <p>{startupSourceLabel(item.source)}</p>
        </div>
        <ObservationBadge observation={item.observation} />
      </header>

      <dl className="fact-list">
        {item.valueName ? (
          <div>
            <dt>值名称</dt>
            <dd className="breakable mono-text">{item.valueName}</dd>
          </div>
        ) : null}
        <div>
          <dt>值类型</dt>
          <dd>
            {item.valueType ? startupValueTypeLabel(item.valueType) : "不适用（文件夹条目）"}
          </dd>
        </div>
        <div>
          <dt>Raw Value</dt>
          <dd>{item.rawValueReadable ? "可读取" : "无法解码为文本"}</dd>
        </div>
        <div>
          <dt>目标类型</dt>
          <dd>{startupTargetKindLabel(item.targetKind)}</dd>
        </div>
        <div>
          <dt>目标路径</dt>
          <dd className="breakable mono-text">
            {redactUserPath(item.targetPath) ?? "未能解析"}
          </dd>
        </div>
        {item.targetCandidate ? (
          <div>
            <dt>候选路径（未探测）</dt>
            <dd className="breakable mono-text">{redactUserPath(item.targetCandidate)}</dd>
          </div>
        ) : null}
        <div>
          <dt>目标状态</dt>
          <dd>{pathStateLabel(item.targetState)}</dd>
        </div>
        <div>
          <dt>路径解析</dt>
          <dd>{startupPathResolutionLabel(item.pathResolution)}</dd>
        </div>
        {item.targetKind === "shortcut" ? (
          <>
            <div>
              <dt>快捷方式参数</dt>
              <dd>{presenceLabel(item.shortcutArgumentsPresent, "存在（内容未读取）")}</dd>
            </div>
            <div>
              <dt>快捷方式工作目录</dt>
              <dd>
                {presenceLabel(item.shortcutWorkingDirectoryPresent, "存在（内容未读取）")}
              </dd>
            </div>
          </>
        ) : null}
        <div>
          <dt>启动审批</dt>
          <dd>启用状态未核验</dd>
        </div>
        <div>
          <dt>来源</dt>
          <dd>
            {installScopeLabel(item.scope)}
            {item.registryView ? ` · ${registryViewLabel(item.registryView)}` : ""}
          </dd>
        </div>
        {item.registryKey ? (
          <div>
            <dt>注册表位置</dt>
            <dd className="breakable mono-text">{item.registryKey}</dd>
          </div>
        ) : null}
        <div>
          <dt>条目状态</dt>
          <dd>
            <ObservationBadge observation={item.observation} />
          </dd>
        </div>
      </dl>

      {item.issues.length > 0 ? (
        <div className="startup-item-issues">
          <h4>本条目读取问题</h4>
          <ul>
            {item.issues.map((issue) => (
              <li key={`${issue.code}:${issue.message}`}>
                <code>{issue.code}</code>
                <span>{issue.message}</span>
              </li>
            ))}
          </ul>
        </div>
      ) : null}

      <div className="startup-static-clue">
        <h4>静态线索（非恶意判定）</h4>
        <p>
          <RiskBadge level={item.risk.level} /> {item.risk.summary}
        </p>
        <p className="fine-print">{item.risk.recommendation}</p>
        <p className="fine-print">
          规则：{item.risk.ruleIds.join("、") || "无"}。该线索只用于提示人工核实，不是恶意或安全结论。
        </p>
      </div>

      <div className="caveat-box">
        <strong>不要仅凭启动线索定罪</strong>
        <p>
          启动项存在并不自动代表恶意，必须结合发布者、签名、来源、用户意图和运行行为核实。
          本阶段只读，不会禁用、删除或执行启动项。
        </p>
      </div>
    </div>
  );
}
