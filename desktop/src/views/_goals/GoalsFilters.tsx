/**
 * The Goals screen's filters — the only narrowing there is, because there
 * are no sections: who holds the ball, which workflow, tags, text. Holder
 * and workflow and text live in the URL (`?holder=&workflow=&q=`), so Back
 * restores a view and a link carries it; tags reuse the shared bar.
 */
import { SegmentedControl, Switch, TextInput, type Segment } from "../../ui";
import { HOLDERS, HOLDER_FILTER_ALL, HOLDER_LABEL, type Filters } from "./goalStripModel.mjs";
import { t } from "../../i18n/l10n.mjs";

const HOLDER_SEGMENTS: readonly Segment<string>[] = [
  { id: HOLDER_FILTER_ALL, label: t("goals-goals-filters-all") },
  ...HOLDERS.map((h) => ({ id: h, label: HOLDER_LABEL[h] })),
];

export function GoalsFilters({
  filters,
  facets,
  onChange,
}: {
  filters: Filters;
  facets: { name: string; count: number }[];
  onChange: (next: Filters) => void;
}) {
  return (
    <div className="flex min-w-0 flex-wrap items-center gap-2">
      <SegmentedControl
        options={HOLDER_SEGMENTS}
        value={filters.holder ?? HOLDER_FILTER_ALL}
        onChange={(h) => onChange({ ...filters, holder: h === HOLDER_FILTER_ALL ? undefined : h })}
        label={t("goals-goals-filters-who-holds")}
      />
      {facets.length > 1 && (
        <select
          aria-label={t("goals-goals-filters-workflow")}
          className="rounded-control border border-border bg-surface px-1.5 py-1 text-2xs"
          value={filters.workflow ?? ""}
          onChange={(e) => onChange({ ...filters, workflow: e.target.value || undefined })}
        >
          <option value="">{t("goals-goals-filters-every-workflow")}</option>
          {facets.map((f) => (
            <option key={f.name} value={f.name}>
              {f.name} ({f.count})
            </option>
          ))}
        </select>
      )}
      <TextInput
        aria-label={t("goals-goals-filters-filter-goals")}
        placeholder={t("goals-goals-filters-filter")}
        value={filters.q ?? ""}
        onChange={(e) => onChange({ ...filters, q: e.target.value || undefined })}
        className="h-7 w-40 text-2xs"
      />
      <Switch checked={!!filters.archived} onChange={(on) => onChange({ ...filters, archived: on || undefined })} label={t("goals-goals-filters-archived")} />
    </div>
  );
}
