/**
 * The Goals screen's filters — the only narrowing there is, because there
 * are no sections: who holds the ball, which workflow, tags, text. Holder
 * and workflow and text live in the URL (`?holder=&workflow=&q=`), so Back
 * restores a view and a link carries it; tags reuse the shared bar.
 *
 * Who holds it is a select, not a segmented control: six options, each a
 * phrase, are past what a row of segments carries (the kit's rule is two to
 * four). Its words are the filter's own, sentence case
 * (`HOLDER_FILTER_LABEL`).
 */
import { Select, Switch, TextInput } from "../../ui";
import { HOLDERS, HOLDER_FILTER_ALL, HOLDER_FILTER_LABEL, type Filters } from "./goalStripModel.mjs";
import { t } from "../../i18n/l10n.mjs";

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
      <Select
        aria-label={t("goals-goals-filters-who-holds")}
        className="h-7 w-auto py-0 text-2xs"
        value={filters.holder ?? HOLDER_FILTER_ALL}
        onChange={(e) => onChange({ ...filters, holder: e.target.value === HOLDER_FILTER_ALL ? undefined : e.target.value })}
      >
        <option value={HOLDER_FILTER_ALL}>{t("goals-goals-filters-all")}</option>
        {HOLDERS.map((h) => (
          <option key={h} value={h}>
            {HOLDER_FILTER_LABEL[h]}
          </option>
        ))}
      </Select>
      {facets.length > 1 && (
        <Select
          aria-label={t("goals-goals-filters-workflow")}
          className="h-7 w-auto py-0 text-2xs"
          value={filters.workflow ?? ""}
          onChange={(e) => onChange({ ...filters, workflow: e.target.value || undefined })}
        >
          <option value="">{t("goals-goals-filters-every-workflow")}</option>
          {facets.map((f) => (
            <option key={f.name} value={f.name}>
              {f.name} ({f.count})
            </option>
          ))}
        </Select>
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
