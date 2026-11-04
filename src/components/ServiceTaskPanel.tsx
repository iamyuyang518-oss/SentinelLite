import { useId, useState } from "react";

import { useSentinelApi } from "./ApiProvider";
import { DataNotes } from "./DataNotes";
import { InventoryIncompleteNotice } from "./InventoryIncompleteNotice";
import { IssueNote } from "./IssueNote";
import { ListPagination, paginateItems } from "./ListPagination";
import { formatCheckedAt } from "../lib/presentation";
import { redactAccountName, redactAuthorName, redactUserPath } from "../lib/privacy";
import { useScanAction, type ScanState } from "../lib/useScanAction";
import {
  inventoryObservationLabel,
  scheduledTaskActionTypeLabel,
  scheduledTaskStateLabel,
  serviceStartTypeLabel,
  serviceStateLabel,
  serviceTypeLabel,
} from "../lib/uiLabels";
import type {
  InventoryObservation,
  ScheduledTaskInventoryResult,
  ScheduledTaskSummary,
  ServiceInventoryResult,
  ServiceSummary,
} from "../types";

function InventoryBadge({ observation }: { observation: InventoryObservation }) {
  return (
    <span className={`inventory-badge inventory-badge--${observation}`}>
      {inventoryObservationLabel(observation)}
    </span>
  );
}

function ServiceSection({ state }: { state: ScanState<ServiceInventoryResult> }) {
  const [query, setQuery] = useState("");
  const [page, setPage] = useState(1);
  const searchId = useId();

  const data = state.phase === "ready" ? state.data : null;
  const normalizedQuery = query.trim().toLowerCase();
  const filtered =
    data?.items.filter((item) => {
      if (normalizedQuery.length === 0) {
        return true;
      }
      return [
        item.name,
        item.displayName,
        redactAccountName(item.account),
        redactUserPath(item.executablePath),
      ].some((value) => value?.toLowerCase().includes(normalizedQuery));
    }) ?? [];
  const { items, page: currentPage, pageCount } = paginateItems(filtered, page);

  return (
    <section className="inventory-section" aria-labelledby="service-inventory-heading">
      <header className="inventory-section__header">
        <div>
          <h3 id="service-inventory-heading">Windows 服务</h3>
          <p>服务控制管理器（SCM）只读快照；不启动、停止、创建、删除或修改服务。</p>
        </div>
        {data ? <InventoryBadge observation={data.observation} /> : null}
      </header>

      {state.phase === "idle" ? (
        <p className="inventory-idle">尚未读取服务快照。点击上方“开始只读盘点”读取。</p>
      ) : null}
      {state.phase === "loading" ? (
        <p className="inventory-idle" role="status">
          正在读取服务列表…
        </p>
      ) : null}
      {state.phase === "error" ? (
        <div className="notice notice--error" role="alert">
          <h4>服务列表未读取</h4>
          <p>{state.message}</p>
          <p className="fine-print">未保留上一次结果；空列表不代表系统中没有服务。</p>
        </div>
      ) : null}

      {data ? (
        <>
          {data.truncated ? (
            <InventoryIncompleteNotice scope="服务" issues={data.issues} />
          ) : null}

          <div className="scan-summary">
            <div>
              <span className="scan-summary__value">{data.items.length}</span>
              <span>{data.truncated ? "个已收集服务（不完整）" : "个服务"}</span>
            </div>
            <p>读取时间：{formatCheckedAt(data.checkedAt)}</p>
            <p>
              {data.observation === "observed"
                ? "本次范围内未标记读取失败；这不代表服务配置正常或安全。"
                : "存在部分未知或读取失败；请查看条目状态和读取问题。"}
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
                placeholder="服务名、显示名、账户或路径"
                autoComplete="off"
              />
            </label>
            <span className="list-count">
              显示 {items.length} / {filtered.length}
            </span>
          </div>

          {items.length > 0 ? (
            <div
              className="table-scroll"
              role="region"
              aria-label="Windows 服务列表（可横向滚动）"
              tabIndex={0}
            >
              <table className="data-table inventory-table" aria-label="Windows 服务列表">
                <thead>
                  <tr>
                    <th scope="col">服务</th>
                    <th scope="col">状态 / 启动</th>
                    <th scope="col">账户</th>
                    <th scope="col">可执行文件</th>
                    <th scope="col">观察</th>
                  </tr>
                </thead>
                <tbody>
                  {items.map((item) => (
                    <ServiceRow key={item.id} item={item} />
                  ))}
                </tbody>
              </table>
            </div>
          ) : (
            <div className="empty-block">
              <p>
                {data.items.length === 0
                  ? "服务枚举没有返回可显示条目。"
                  : "当前筛选没有匹配项。"}
              </p>
              <p>空列表不代表系统中没有服务，也不代表服务配置正常。</p>
            </div>
          )}

          <ListPagination
            page={currentPage}
            pageCount={pageCount}
            onPageChange={setPage}
            label="Windows 服务分页"
          />
          <DataNotes limitations={data.limitations} issues={data.issues} />
        </>
      ) : null}
    </section>
  );
}

function ServiceRow({ item }: { item: ServiceSummary }) {
  return (
    <tr>
      <td>
        <span className="row-title">{item.name}</span>
        <span className="cell-note">{item.displayName}</span>
        <span className="cell-note">
          {item.serviceTypes.map(serviceTypeLabel).join(" / ")}
        </span>
      </td>
      <td>
        <span>{serviceStateLabel(item.state)}</span>
        <span className="cell-note">{serviceStartTypeLabel(item.startType)}</span>
      </td>
      <td>
        <span className={item.account === null ? "cell-note" : "mono-text breakable"}>
          {redactAccountName(item.account) ??
            (item.observation === "observed" ? "未提供" : "未知")}
        </span>
      </td>
      <td>
        <span
          className={
            item.executablePath === null ? "cell-note" : "mono-text breakable"
          }
        >
          {redactUserPath(item.executablePath) ??
            (item.observation === "observed" ? "未提供" : "未知")}
        </span>
      </td>
      <td>
        <InventoryBadge observation={item.observation} />
        <IssueNote issues={item.issues} />
      </td>
    </tr>
  );
}

function TaskSection({ state }: { state: ScanState<ScheduledTaskInventoryResult> }) {
  const [query, setQuery] = useState("");
  const [page, setPage] = useState(1);
  const searchId = useId();

  const data = state.phase === "ready" ? state.data : null;
  const normalizedQuery = query.trim().toLowerCase();
  const filtered =
    data?.items.filter((item) => {
      if (normalizedQuery.length === 0) {
        return true;
      }
      return [
        item.name,
        item.path,
        redactAuthorName(item.author),
        redactAccountName(item.runAs),
        ...item.actions.map((action) => redactUserPath(action.executablePath)),
      ].some((value) => value?.toLowerCase().includes(normalizedQuery));
    }) ?? [];
  const { items, page: currentPage, pageCount } = paginateItems(filtered, page);

  return (
    <section className="inventory-section" aria-labelledby="task-inventory-heading">
      <header className="inventory-section__header">
        <div>
          <h3 id="task-inventory-heading">计划任务</h3>
          <p>任务计划程序只读快照；不运行、创建、修改、删除、启用或禁用任务。</p>
        </div>
        {data ? <InventoryBadge observation={data.observation} /> : null}
      </header>

      {state.phase === "idle" ? (
        <p className="inventory-idle">尚未读取计划任务快照。点击上方“开始只读盘点”读取。</p>
      ) : null}
      {state.phase === "loading" ? (
        <p className="inventory-idle" role="status">
          正在读取计划任务列表…
        </p>
      ) : null}
      {state.phase === "error" ? (
        <div className="notice notice--error" role="alert">
          <h4>计划任务列表未读取</h4>
          <p>{state.message}</p>
          <p className="fine-print">未保留上一次结果；空列表不代表系统中没有计划任务。</p>
        </div>
      ) : null}

      {data ? (
        <>
          {data.truncated ? (
            <InventoryIncompleteNotice scope="计划任务" issues={data.issues} />
          ) : null}

          <div className="scan-summary">
            <div>
              <span className="scan-summary__value">{data.items.length}</span>
              <span>{data.truncated ? "个已收集任务（不完整）" : "个计划任务"}</span>
            </div>
            <p>读取时间：{formatCheckedAt(data.checkedAt)}</p>
            <p>
              {data.observation === "observed"
                ? "本次范围内未标记读取失败；这不代表任务内容或配置安全。"
                : "存在部分未知或读取失败；请查看条目状态和读取问题。"}
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
                placeholder="任务名、路径、作者、运行身份或程序"
                autoComplete="off"
              />
            </label>
            <span className="list-count">
              显示 {items.length} / {filtered.length}
            </span>
          </div>

          {items.length > 0 ? (
            <div
              className="table-scroll"
              role="region"
              aria-label="计划任务列表（可横向滚动）"
              tabIndex={0}
            >
              <table className="data-table inventory-table inventory-table--tasks" aria-label="计划任务列表">
                <thead>
                  <tr>
                    <th scope="col">任务</th>
                    <th scope="col">状态</th>
                    <th scope="col">主体</th>
                    <th scope="col">动作</th>
                    <th scope="col">观察</th>
                  </tr>
                </thead>
                <tbody>
                  {items.map((item) => (
                    <TaskRow key={item.id} item={item} />
                  ))}
                </tbody>
              </table>
            </div>
          ) : (
            <div className="empty-block">
              <p>
                {data.items.length === 0
                  ? "任务枚举没有返回可显示条目。"
                  : "当前筛选没有匹配项。"}
              </p>
              <p>空列表不代表系统中没有计划任务，也不代表任务配置正常。</p>
            </div>
          )}

          <ListPagination
            page={currentPage}
            pageCount={pageCount}
            onPageChange={setPage}
            label="计划任务分页"
          />
          <DataNotes limitations={data.limitations} issues={data.issues} />
        </>
      ) : null}
    </section>
  );
}

function ActionList({ actions }: { actions: ScheduledTaskSummary["actions"] }) {
  return (
    <ul className="inventory-actions">
      {actions.map((action, index) => (
        <li key={`${action.actionType}-${index}`}>
          <span>{scheduledTaskActionTypeLabel(action.actionType)}</span>
          <span
            className={
              action.executablePath === null ? "cell-note" : "mono-text breakable"
            }
          >
            {redactUserPath(action.executablePath) ?? "无可执行路径"}
          </span>
        </li>
      ))}
    </ul>
  );
}

function TaskRow({ item }: { item: ScheduledTaskSummary }) {
  const enabledLabel =
    item.enabled === null ? "启用状态未知" : item.enabled ? "已启用" : "未启用";
  const visibleActions = item.actions.slice(0, 3);
  const remainingActions = item.actions.slice(3);
  return (
    <tr>
      <td>
        <span className="row-title">{item.name}</span>
        <span className="cell-note mono-text">{item.path}</span>
      </td>
      <td>
        <span>{scheduledTaskStateLabel(item.state)}</span>
        <span className="cell-note">{enabledLabel}</span>
      </td>
      <td>
        <span className={item.author === null ? "cell-note" : "breakable"}>
          {redactAuthorName(item.author) ??
            (item.observation === "observed" ? "作者未提供" : "作者未知")}
        </span>
        <span className={item.runAs === null ? "cell-note" : "mono-text breakable"}>
          {redactAccountName(item.runAs) ??
            (item.observation === "observed" ? "运行身份未提供" : "运行身份未知")}
        </span>
      </td>
      <td>
        {visibleActions.length > 0 ? (
          <ActionList actions={visibleActions} />
        ) : (
          <span className="cell-note">
            {item.observation === "observed" ? "未提供动作" : "动作未知"}
          </span>
        )}
        {remainingActions.length > 0 ? (
          <details className="inventory-actions-more">
            <summary>查看其余 {remainingActions.length} 个动作</summary>
            <ActionList actions={remainingActions} />
          </details>
        ) : null}
        {item.actionsTruncated ? (
          <span className="cell-note cell-note--warning">
            动作超过显示上限，未显示全部动作
          </span>
        ) : null}
      </td>
      <td>
        <InventoryBadge observation={item.observation} />
        <IssueNote issues={item.issues} />
      </td>
    </tr>
  );
}

export function ServiceTaskPanel({
  section = "all",
}: {
  section?: "all" | "services" | "tasks";
}) {
  const api = useSentinelApi();
  const services = useScanAction(api.listServices);
  const tasks = useScanAction(api.listScheduledTasks);
  const headingId = useId();
  const showServices = section !== "tasks";
  const showTasks = section !== "services";
  const loading =
    (showServices && services.state.phase === "loading") ||
    (showTasks && tasks.state.phase === "loading");

  function runScan(): void {
    if (showServices) {
      void services.start();
    }
    if (showTasks) {
      void tasks.start();
    }
  }

  const ready =
    (showServices && services.state.phase === "ready") ||
    (showTasks && tasks.state.phase === "ready");
  const failed =
    (showServices && services.state.phase === "error") ||
    (showTasks && tasks.state.phase === "error");
  const label = loading ? "正在读取…" : ready ? "重新盘点" : failed ? "重新尝试" : "开始只读盘点";
  const title =
    section === "services"
      ? "Windows 服务"
      : section === "tasks"
        ? "计划任务"
        : "Windows 服务与计划任务";

  return (
    <section className="scan-panel service-task-panel" aria-labelledby={headingId}>
      <header className="scan-panel__header">
        <div>
          <h2 id={headingId}>{title}</h2>
          <p>只读枚举服务与计划任务，用于软件关联分析。</p>
        </div>
        <button
          type="button"
          className="button button--primary"
          onClick={runScan}
          disabled={loading}
        >
          {label}
        </button>
      </header>

      <div className="caveat-box">
        <strong>只读盘点，不执行任何条目</strong>
        <p>
          不启动/停止服务，不创建/删除/修改服务，不运行/创建/修改/删除/启用/禁用计划任务。
          只显示可执行文件路径，不读取服务启动参数、任务 Arguments、WorkingDirectory 或触发器；
          结果仅在内存中展示，不写入 SQLite。无权限或字段缺失会保持未知，不会被当作正常。
        </p>
      </div>

      <div className="inventory-stack">
        {showServices ? <ServiceSection state={services.state} /> : null}
        {showTasks ? <TaskSection state={tasks.state} /> : null}
      </div>
    </section>
  );
}
