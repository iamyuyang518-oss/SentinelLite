import { useId, useState } from "react";

import { useSentinelApi } from "./ApiProvider";
import { DataNotes } from "./DataNotes";
import { InventoryIncompleteNotice } from "./InventoryIncompleteNotice";
import { IssueNote } from "./IssueNote";
import { ListPagination, paginateItems } from "./ListPagination";
import { RiskBadge } from "./RiskBadge";
import { SoftwareAssociations } from "./SoftwareAssociations";
import { formatCheckedAt } from "../lib/presentation";
import { redactUserPath } from "../lib/privacy";
import {
  installScopeLabel,
  inventoryObservationLabel,
  pathStateLabel,
  registryValueStateLabel,
  registryViewLabel,
  softwareClassificationLabel,
} from "../lib/uiLabels";
import {
  useScanAction,
  type ScanController,
  type ScanState,
} from "../lib/useScanAction";
import type {
  AssetAssociationResult,
  InstalledSoftware,
  InventoryObservation,
  RegistryValueState,
  SoftwareSourceStatus,
} from "../types";

function ObservationBadge({ observation }: { observation: InventoryObservation }) {
  return (
    <span className={`inventory-badge inventory-badge--${observation}`}>
      {inventoryObservationLabel(observation)}
    </span>
  );
}

function sourceStatusLabel(source: SoftwareSourceStatus): string {
  if (source.exists === false) {
    return "不存在";
  }
  return inventoryObservationLabel(source.observation);
}

function sourceStatusClass(source: SoftwareSourceStatus): string {
  return source.exists === false ? "notPresent" : source.observation;
}

function formatOptionalField(
  value: string | null,
  state: RegistryValueState,
): string {
  if (state !== "present") {
    return registryValueStateLabel(state);
  }
  return value?.trim() ? value : "已读取（空值）";
}

function formatInstallDate(value: string | null, state: RegistryValueState): string {
  if (state !== "present") {
    return registryValueStateLabel(state);
  }
  return value ?? "已读取（空值）";
}

function formatNumberField(value: number | null, state: RegistryValueState): string {
  if (state !== "present") {
    return registryValueStateLabel(state);
  }
  if (value === null) {
    return "已读取（空值）";
  }
  return value === 0 ? "否（0）" : `是（${value}）`;
}

function formatStringField(value: string | null, state: RegistryValueState): string {
  if (state !== "present") {
    return registryValueStateLabel(state);
  }
  return value?.trim() ? value : "已读取（空值）";
}

function isUnreadableState(state: RegistryValueState): boolean {
  return state === "typeMismatch" || state === "invalid" || state === "readError";
}

export function SoftwarePanel({
  association,
}: {
  association: ScanController<AssetAssociationResult>;
}) {
  const api = useSentinelApi();
  const { state, start } = useScanAction(api.scanInstalledSoftware);
  const [query, setQuery] = useState("");
  const [page, setPage] = useState(1);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const searchId = useId();

  function runScan(): void {
    association.reset();
    setSelectedId(null);
    setPage(1);
    void start();
  }

  const data = state.phase === "ready" ? state.data : null;
  const sortedItems = data
    ? [...data.items].sort((left, right) =>
        left.displayName.localeCompare(right.displayName, "zh-CN"),
      )
    : [];

  const normalizedQuery = query.trim().toLowerCase();
  const filteredItems = sortedItems.filter((item) => {
    if (normalizedQuery.length === 0) {
      return true;
    }
    return [
      item.displayName,
      item.version,
      item.publisher,
      item.installLocation,
      item.displayIconPath,
      item.registryKey,
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
          : "开始只读盘点";

  return (
    <section className="scan-panel" aria-labelledby="software-panel-heading">
      <header className="scan-panel__header">
        <div>
          <h2 id="software-panel-heading">软件与组件只读盘点</h2>
          <p>
            读取传统 Uninstall 注册信息、发布者、安装位置和静态图标线索。只读操作，
            不调用卸载程序，不展示完整卸载命令。
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
          正在读取传统 Uninstall 注册表来源；不会调用卸载程序或修改注册表。
        </p>
      ) : null}

      {state.phase === "error" ? (
        <div className="notice notice--error" role="alert">
          <div>
            <h3>软件盘点未完成</h3>
            <p>{state.message}</p>
            <p className="fine-print">
              未保留上一次结果；空列表或读取失败都不表示系统没有安装软件。
            </p>
          </div>
        </div>
      ) : null}

      {data ? (
        <>
          {data.truncated ? (
            <InventoryIncompleteNotice scope="软件盘点" issues={data.issues} />
          ) : null}

          <div className="scan-summary">
            <div>
              <span className="scan-summary__value">{data.items.length}</span>
              <span>{data.truncated ? "个可显示注册项（不完整）" : "个可显示注册项"}</span>
            </div>
            <ObservationBadge observation={data.observation} />
            <p>读取时间：{formatCheckedAt(data.checkedAt)}</p>
            <p>
              {data.observation === "observed"
                ? "本次覆盖的来源未标记读取失败；这不代表软件安全、已启用或身份一定唯一。"
                : "存在部分未知或读取失败；请查看来源状态、条目状态和读取问题。"}
            </p>
          </div>

          <div className="caveat-box">
            <strong>盘点范围不完整</strong>
            <p>
              Microsoft Store 应用、MSIX、便携软件、服务、计划任务、签名与卸载残留尚未纳入本页。
              软件名称、发布者和路径不能单独作为恶意或安全结论。
            </p>
          </div>

          <section className="software-sources" aria-labelledby="software-sources-heading">
            <div className="software-sources__header">
              <h3 id="software-sources-heading">数据来源</h3>
              <p>每个注册表视图独立读取；一个来源失败不会清空其他来源。</p>
            </div>
            <div className="software-source-grid">
              {data.sources.map((source) => (
                <article className="software-source-card" key={source.id}>
                  <header>
                    <strong>Uninstall 注册表</strong>
                    <span
                      className={`inventory-badge inventory-badge--${sourceStatusClass(source)}`}
                    >
                      {sourceStatusLabel(source)}
                    </span>
                  </header>
                  <p>
                    {installScopeLabel(source.scope)} · {registryViewLabel(source.registryView)}
                  </p>
                  <p className="mono-text breakable">{source.registryKey}</p>
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
                placeholder="软件名、版本、发布者或安装路径"
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
                  aria-label="已安装软件列表（可横向滚动）"
                  tabIndex={0}
                >
                  <table
                    className="data-table inventory-table software-table"
                    aria-label="已安装软件列表"
                  >
                    <thead>
                      <tr>
                        <th scope="col">软件 / 版本 / 发布者</th>
                        <th scope="col">来源 / 范围</th>
                        <th scope="col">安装位置</th>
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
                              aria-label={`查看 ${item.displayName} 的详情`}
                              aria-expanded={item.id === selectedId}
                              aria-controls="software-detail-panel"
                              onClick={() => setSelectedId(item.id)}
                            >
                              {item.displayName}
                            </button>
                            <span className="cell-note">
                              版本：{formatOptionalField(item.version, item.versionState)} ·
                              发布者：
                              {formatOptionalField(item.publisher, item.publisherState)}
                            </span>
                            {item.classification !== "userSoftware" ? (
                              <span className="cell-note classification-note">
                                类别：{softwareClassificationLabel(item.classification)}
                              </span>
                            ) : null}
                            <IssueNote issues={item.issues} />
                          </td>
                          <td>
                            {installScopeLabel(item.scope)}
                            <span className="cell-note">{registryViewLabel(item.registryView)}</span>
                          </td>
                          <td>
                            <span className="mono-text breakable">
                              {item.installLocationValueState === "present"
                                ? (redactUserPath(item.installLocation) ?? "已读取（空值）")
                                : registryValueStateLabel(item.installLocationValueState)}
                            </span>
                            <span className="cell-note">
                              {pathStateLabel(item.installLocationState)}
                            </span>
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
                      ? "软件盘点没有返回可显示的传统卸载条目。"
                      : "当前筛选没有匹配项。"}
                  </p>
                  <p>空列表或未匹配不代表系统没有安装软件。</p>
                </div>
              )}

              <ListPagination
                page={currentPage}
                pageCount={pageCount}
                onPageChange={setPage}
                label="已安装软件分页"
              />
            </div>

            <aside
              id="software-detail-panel"
              className="detail-column"
              role="region"
              aria-label="软件详情"
              aria-live="polite"
            >
              {selected ? (
                <SoftwareDetail
                  item={selected}
                  associationState={association.state}
                  onStartAssociation={() => void association.start()}
                />
              ) : (
                <div className="detail-placeholder">
                  <h3>证据面板</h3>
                  <p>选择一项软件，查看它的元数据、来源、观察状态和已知盲区。</p>
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

function SoftwareDetail({
  item,
  associationState,
  onStartAssociation,
}: {
  item: InstalledSoftware;
  associationState: ScanState<AssetAssociationResult>;
  onStartAssociation: () => void;
}) {
  const componentNotice =
    item.classification === "userSoftware"
      ? null
      : item.classification === "unknown"
        ? "类别未知：可能是组件、更新或普通软件，当前证据不足。"
        : "这是系统组件或组件类条目，不是普通用户软件。";
  const unreadableFields = [
    ["DisplayVersion", item.versionState],
    ["Publisher", item.publisherState],
    ["InstallLocation", item.installLocationValueState],
    ["InstallDate", item.installDateState],
    ["DisplayIcon", item.displayIconState],
    ["WindowsInstaller", item.windowsInstallerState],
    ["SystemComponent", item.systemComponentState],
    ["NoRemove", item.noRemoveState],
    ["ParentKeyName", item.parentKeyNameState],
    ["ReleaseType", item.releaseTypeState],
  ].filter((entry) => isUnreadableState(entry[1] as RegistryValueState));

  return (
    <div className="detail-card">
      <header className="detail-card__header">
        <div>
          <p className="detail-overline">软件条目</p>
          <h3>{item.displayName}</h3>
          <p>发布者：{formatOptionalField(item.publisher, item.publisherState)}</p>
        </div>
        <ObservationBadge observation={item.observation} />
      </header>

      {componentNotice ? (
        <div className="component-notice">
          <strong>{softwareClassificationLabel(item.classification)}</strong>
          <p>{componentNotice}</p>
        </div>
      ) : null}

      {unreadableFields.length > 0 ? (
        <div className="component-notice">
          <strong>字段读取异常</strong>
          <p>
            {unreadableFields.map(([name]) => name).join("、")} 无法按预期类型读取；
            分类保持保守，不会回退为普通用户软件。
          </p>
        </div>
      ) : null}

      <dl className="fact-list">
        <div>
          <dt>分类</dt>
          <dd>{softwareClassificationLabel(item.classification)}</dd>
        </div>
        <div>
          <dt>版本</dt>
          <dd>{formatOptionalField(item.version, item.versionState)}</dd>
        </div>
        <div>
          <dt>发布者</dt>
          <dd>{formatOptionalField(item.publisher, item.publisherState)}</dd>
        </div>
        <div>
          <dt>安装时间</dt>
          <dd>{formatInstallDate(item.installDate, item.installDateState)}</dd>
        </div>
        <div>
          <dt>来源</dt>
          <dd>
            {installScopeLabel(item.scope)} · {registryViewLabel(item.registryView)}
          </dd>
        </div>
        <div>
          <dt>安装位置</dt>
          <dd className="breakable">
            {item.installLocationValueState === "present"
              ? (redactUserPath(item.installLocation) ?? "已读取（空值）")
              : registryValueStateLabel(item.installLocationValueState)}
          </dd>
        </div>
        <div>
          <dt>路径状态</dt>
          <dd>{pathStateLabel(item.installLocationState)}</dd>
        </div>
        <div>
          <dt>DisplayIcon</dt>
          <dd className="breakable">
            {item.displayIconPath
              ? redactUserPath(item.displayIconPath)
              : item.displayIconState === "present"
                ? "已读取（无可解析路径）"
                : registryValueStateLabel(item.displayIconState)}
          </dd>
        </div>
        <div>
          <dt>WindowsInstaller</dt>
          <dd>{formatNumberField(item.windowsInstaller, item.windowsInstallerState)}</dd>
        </div>
        <div>
          <dt>SystemComponent</dt>
          <dd>{formatNumberField(item.systemComponent, item.systemComponentState)}</dd>
        </div>
        <div>
          <dt>NoRemove</dt>
          <dd>{formatNumberField(item.noRemove, item.noRemoveState)}</dd>
        </div>
        <div>
          <dt>ParentKeyName</dt>
          <dd className="breakable">
            {formatStringField(item.parentKeyName, item.parentKeyNameState)}
          </dd>
        </div>
        <div>
          <dt>ReleaseType</dt>
          <dd>{formatStringField(item.releaseType, item.releaseTypeState)}</dd>
        </div>
        <div>
          <dt>卸载入口</dt>
          <dd>{item.uninstallCommandPresent ? "存在（内容未读取）" : "未提供或为空"}</dd>
        </div>
        <div>
          <dt>静默卸载入口</dt>
          <dd>
            {item.quietUninstallCommandPresent ? "存在（内容未读取）" : "未提供或为空"}
          </dd>
        </div>
        <div>
          <dt>条目状态</dt>
          <dd>
            <ObservationBadge observation={item.observation} />
          </dd>
        </div>
        <div>
          <dt>注册表来源</dt>
          <dd className="breakable mono-text">{item.registryKey}</dd>
        </div>
      </dl>

      {item.issues.length > 0 ? (
        <div className="software-item-issues">
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

      <div className="software-static-clue">
        <h4>静态线索（非恶意判定）</h4>
        <p>
          <RiskBadge level={item.risk.level} /> {item.risk.summary}
        </p>
        <p className="fine-print">{item.risk.recommendation}</p>
        <p className="fine-print">
          规则：{item.risk.ruleIds.join("、") || "无"}。软件名称、发布者和路径不能单独作为恶意或安全结论。
        </p>
      </div>

      <SoftwareAssociations
        softwareId={item.id}
        state={associationState}
        onStart={onStartAssociation}
      />

      <p className="read-only-note">
        本阶段不会卸载软件或删除残留。后续处置必须经过操作预览、明确确认和可验证的恢复方案。
      </p>
    </div>
  );
}
