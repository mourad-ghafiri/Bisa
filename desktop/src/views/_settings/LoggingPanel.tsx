/**
 * Settings › Node › Logging: the diagnostic log on this machine — the
 * newest crash first, then the folder and what is in it, one block per
 * process family and the crash reports, then the four `logging.*` controls
 * the registry grows.
 *
 * The files are the node's, the CLI's, the MCP servers' and the desktop's
 * own words about themselves (`bisa-log`), read back through
 * `GET /logs`; nothing here shows a line of a file — a crash report is the
 * one thing read whole (`GET /logs/crashes/{name}`), because it is what a
 * person needs first — and the panel says once that nothing leaves this
 * machine. The controls render through the generic `RegistryPanel` — a
 * `logging.*` key added in `bisa-core` grows a control here with no
 * change to this file.
 */

import { useMemo, useState } from "react";
import { api, inDesktopShell, revealPath } from "../../api";
import { useEngineEvents } from "../../bus";
import { Button, Card, CopyText, ErrorNote, Pending, Section, revealLabel, useToast } from "../../ui";
import type { CrashReportView, CrashSummaryView } from "../../types";
import { attempt, useAsync } from "../_work/useAsync";
import { pendingRows } from "./loadModel.mjs";
import { RegistryPanel } from "./RegistryPanel";
import { LOCAL_ONLY, crashRows, familyRows, latestCrashWords, totalWords } from "./loggingModel.mjs";
import { t } from "../../i18n/l10n.mjs";
import { rich } from "../../i18n/rich";

export function LoggingPanel() {
  const toast = useToast();
  const logs = useAsync((s) => api.logs(s), []);
  useEngineEvents((e) => {
    if (e.payload.type === "settings_changed" && e.payload.keys.some((k) => k.startsWith("logging."))) logs.reload();
  });
  const desktop = inDesktopShell();
  const families = useMemo(() => familyRows(logs.data?.families ?? []), [logs.data]);
  const crashes = useMemo(() => crashRows(logs.data?.crashes ?? []), [logs.data]);
  const latestWords = useMemo(() => latestCrashWords(logs.data?.latest_crash), [logs.data]);
  const dir = logs.data?.dir;
  const crashesDir = logs.data?.crashes_dir;

  const reveal = (path: string | undefined) => {
    if (!path) return;
    void attempt(() => revealPath(path), toast.error);
  };

  return (
    <div className="flex max-w-2xl flex-col gap-4">
      {logs.data?.latest_crash && latestWords && (
        <Section title={t("settings-logging-panel-last-crash")}>
          <LatestCrash
            summary={logs.data.latest_crash}
            words={latestWords}
            onReveal={desktop && crashesDir ? () => reveal(`${crashesDir}/${logs.data?.latest_crash?.name}`) : undefined}
          />
        </Section>
      )}

      <Section title={t("settings-logging-panel-machine")}>
        <Card>
          <p className="text-2xs text-text-dim">{LOCAL_ONLY}</p>
          {logs.error && <ErrorNote error={logs.error} retry={logs.reload} />}
          {!logs.data && !logs.error && <Pending what={t("settings-load-log-files")} rows={pendingRows(t("settings-load-log-files"))} className="mt-2" />}
          {logs.data && (
            <>
              <div className="mt-2 flex flex-wrap items-center gap-2">
                <CopyText value={logs.data.dir} label={t("settings-logging-panel-log-folder")} />
                <Button size="sm" variant="ghost" className="ml-auto" onClick={() => logs.reload()}>{t("settings-catalog-panel-refresh")}</Button>
                {desktop && (
                  <Button size="sm" onClick={() => reveal(dir)}>
                    {revealLabel(navigator.userAgent)}
                  </Button>
                )}
              </div>
              {families.map((family) => (
                <div key={family.process} className="mt-3">
                  <div className="flex items-center gap-2">
                    <span className="text-2xs font-medium text-text">
                      {rich("settings-logging-panel-family-whose", { process: <span className="font-mono">{family.process}/</span> }, { whose: family.whose })}
                    </span>
                    {family.files.length === 0 && <span className="text-2xs text-text-dim">{t("settings-logging-panel-nothing-yet")}</span>}
                  </div>
                  {family.files.length > 0 && (
                    <div className="mt-1 overflow-x-auto">
                      <table className="w-full text-left text-2xs tabular-nums">
                        <thead className="text-text-dim">
                          <tr>
                            <th className="py-1 pr-3 font-medium">{t("settings-logging-panel-file")}</th>
                            <th className="py-1 pr-3 font-medium">{t("settings-logging-panel-period")}</th>
                            <th className="py-1 pr-3 font-medium">{t("settings-logging-panel-size")}</th>
                            <th className="py-1 font-medium">{t("settings-logging-panel-written")}</th>
                          </tr>
                        </thead>
                        <tbody>
                          {family.files.map((r) => (
                            <tr key={r.name} className="border-t border-border">
                              <td className="py-1 pr-3 font-mono">{r.name}</td>
                              <td className="py-1 pr-3">{r.period}</td>
                              <td className="py-1 pr-3">{r.size}</td>
                              <td className="py-1">{r.age}</td>
                            </tr>
                          ))}
                        </tbody>
                      </table>
                    </div>
                  )}
                </div>
              ))}
              {crashes.length > 0 && (
                <div className="mt-3">
                  <div className="flex items-center gap-2">
                    <span className="text-2xs font-medium text-text">
                      {rich("settings-logging-panel-crashes-folder", { code: (inner) => <span className="font-mono">{inner}</span> })}
                    </span>
                    {desktop && crashesDir && (
                      <Button size="sm" variant="ghost" className="ml-auto" onClick={() => reveal(crashesDir)}>
                        {revealLabel(navigator.userAgent)}
                      </Button>
                    )}
                  </div>
                  <div className="mt-1 overflow-x-auto">
                    <table className="w-full text-left text-2xs tabular-nums">
                      <thead className="text-text-dim">
                        <tr>
                          <th className="py-1 pr-3 font-medium">{t("settings-logging-panel-report")}</th>
                          <th className="py-1 pr-3 font-medium">{t("settings-logging-panel-whose")}</th>
                          <th className="py-1 pr-3 font-medium">{t("settings-logging-panel-when")}</th>
                          <th className="py-1 font-medium">{t("settings-logging-panel-size")}</th>
                        </tr>
                      </thead>
                      <tbody>
                        {crashes.map((r) => (
                          <tr key={r.name} className="border-t border-border">
                            <td className="py-1 pr-3 font-mono">{r.name}</td>
                            <td className="py-1 pr-3">{r.family}</td>
                            <td className="py-1 pr-3">{r.at}</td>
                            <td className="py-1">{r.size}</td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                </div>
              )}
              <p className="mt-2 text-2xs text-text-dim">{totalWords(logs.data)}</p>
            </>
          )}
        </Card>
      </Section>

      <Section title={t("settings-logging-panel-what-written")}>
        <RegistryPanel group="logging" />
      </Section>
    </div>
  );
}

/**
 * The newest report: one sentence, the file's name, and on request the
 * report whole — where a panic was, on which thread, its backtrace, what
 * the node last said, and the last lines the process wrote before.
 */
function LatestCrash({ summary, words, onReveal }: { summary: CrashSummaryView; words: string; onReveal?: () => void }) {
  const [open, setOpen] = useState(false);
  const report = useAsync<CrashReportView | null>((s) => (open ? api.crashReport(summary.name, s) : Promise.resolve(null)), [open, summary.name]);
  return (
    <Card>
      <p className="text-xs font-medium text-text">{words}</p>
      <div className="mt-2 flex flex-wrap items-center gap-2">
        <CopyText value={summary.name} label={t("settings-logging-panel-report-s-name")} />
        <Button size="sm" variant="ghost" className="ml-auto" onClick={() => setOpen((o) => !o)}>
          {open ? t("settings-logging-panel-hide-details") : t("settings-logging-panel-show-details")}
        </Button>
        {onReveal && (
          <Button size="sm" onClick={onReveal}>
            {revealLabel(navigator.userAgent)}
          </Button>
        )}
      </div>
      {open && report.error && <ErrorNote error={report.error} retry={report.reload} />}
      {open && !report.data && !report.error && <Pending what={t("settings-logging-panel-report-2")} rows={pendingRows(t("settings-logging-panel-report-2"))} className="mt-2" />}
      {open && report.data && <CrashDetails report={report.data} />}
    </Card>
  );
}

function CrashDetails({ report }: { report: CrashReportView }) {
  const facts: [string, string][] = [
    [t("settings-logging-panel-fact-process"), t("settings-logging-panel-pid", { process: report.process, version: report.version, pid: report.pid })],
    [t("settings-logging-panel-at"), report.at],
    [t("settings-logging-panel-kind"), report.kind],
  ];
  if (report.location) facts.push([t("settings-logging-panel-where"), report.location]);
  if (report.thread) facts.push([t("settings-logging-panel-thread"), report.thread]);
  if (report.child) {
    const how = report.child.code != null ? t("settings-logging-panel-exit-code", { code: report.child.code }) : report.child.signal != null ? t("settings-logging-panel-signal", { signal: report.child.signal }) : t("settings-logging-panel-how-unknown");
    facts.push([t("settings-logging-panel-fact-child"), t("settings-logging-panel-pid-2", { process: report.child.process, pid: report.child.pid, how })]);
  }
  return (
    <div className="mt-3 flex flex-col gap-2 text-2xs">
      <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1">
        {facts.map(([k, v]) => (
          <div key={k} className="contents">
            <dt className="text-text-dim">{k}</dt>
            <dd className="font-mono">{v}</dd>
          </div>
        ))}
      </dl>
      {report.child && report.child.stderr.length > 0 && (
        <>
          <p className="text-text-dim">{t("settings-logging-panel-what-node-last-said")}</p>
          <pre className="max-h-40 overflow-auto rounded-control bg-surface-2 p-2 text-2xs">{report.child.stderr.join("\n")}</pre>
        </>
      )}
      {report.backtrace && (
        <>
          <p className="text-text-dim">{t("settings-logging-panel-backtrace")}</p>
          <pre className="max-h-60 overflow-auto rounded-control bg-surface-2 p-2 text-2xs">{report.backtrace}</pre>
        </>
      )}
      {report.recent.length > 0 && (
        <>
          <p className="text-text-dim">{t("settings-logging-panel-last-lines-before")}</p>
          <pre className="max-h-60 overflow-auto rounded-control bg-surface-2 p-2 text-2xs">
            {report.recent.map((r) => `${r.at} ${r.level} ${r.target}: ${r.message}${r.fields.length ? " " + r.fields.map(([k, v]) => `${k}=${v}`).join(" ") : ""}`).join("\n")}
          </pre>
        </>
      )}
    </div>
  );
}
