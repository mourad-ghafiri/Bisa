/**
 * The controls a person answers an agent with.
 *
 * One component, every mount point. The Inbox pins it under `NeedsActionCard`,
 * a goal shows it in its *Your move* band and a run of the workspace on its
 * run page — and it decides through the ask's own home (`NeedsAction.home`),
 * the goal or the run, never through the screen it happens to be on. Until
 * now the first two were two files
 * rendering the same two shapes in near-identical code — which is how one of
 * them ended up with a `danger` Decline and the other a ghost one, and how the
 * escape hatch stayed broken in both for as long as it did. The two wrappers
 * now own their chrome (the header line, the glyph, the surround) and nothing
 * else; everything a person can *press* lives here.
 *
 * # Two shapes, and only one of them has three doors
 *
 * A `decision` gate is Approve / Decline with an optional rationale. It is
 * genuinely binary — a signature either happens or it does not — and it gains
 * no third option.
 *
 * An `answer` question gets whatever options the agent offered, a free-text
 * box *beside* them rather than instead of them, and **"I'm not sure"**. The
 * last two are the platform's, not the agent's: an agent that forgets to offer
 * a way out of its own framing would otherwise trap the reader inside it, and
 * that is not a thing to leave to a prompt.
 *
 * "I'm not sure" replaced a link that read "can't answer this" and called
 * `decide(false)` — a decline, which the waiting agent reads as *"The human
 * DENIED. Do not proceed with the questioned action."* The one door out of a
 * question told the agent to stop. It now sends `unsure`, which asks for a
 * narrower question instead, bounded by `max_clarify_rounds`.
 *
 * # The keyboard, and the Inbox's global `j`/`k`
 *
 * The Inbox installs a window-level `j`/`k`/`Enter`/`a` handler that bails on
 * a focused `INPUT`, `TEXTAREA` or `contentEditable` — and on nothing else. A
 * `role="listbox"` of `tabIndex={-1}` buttons, which is how the omnibox and
 * the agent picker draw a list, would therefore move the Inbox's row selection
 * out from under whoever pressed `j` inside it.
 *
 * So the options are **native radio and checkbox inputs**, not a listbox. They
 * come with the roving arrow keys, the group semantics and the space-to-toggle
 * a hand-rolled list has to reimplement, and their `tagName` is `INPUT`, which
 * the Inbox handler already treats as none of its business. The buttons around
 * them are covered by {@link ASK_ATTR}, which the Inbox bails on too.
 *
 * Everything that can be *wrong* here — what body a selection sends, when Send
 * is live, what "I'm not sure" says on the wire — is in `askModel.mjs`, where
 * `node --test` can reach it. This file is paint.
 */

import { useId, useMemo, useState } from "react";
import { api } from "../../api";
import {
  askOptions,
  canSubmit,
  decideBody,
  isAnswerAsk,
  isMulti,
  toggleChoice,
} from "../../askModel.mjs";
import type { InputDef, NeedsAction } from "../../types";
import { Button, Chip, ICON, KeyHint, TextArea, TextInput, cn, useToast } from "../../ui";
import { showHookSecrets } from "../_workflow/hookSecretsStore";
import { InputsForm } from "../_workflow/InputsForm";
import { initialValues, toRequest, validateInputs, type InputValues } from "../_workflow/workflowForm.mjs";
import { t } from "../../i18n/l10n.mjs";

/**
 * The control the Inbox's `a` shortcut focuses, resolved with a single
 * `document.querySelector`. Exactly one element may carry it, which is why it
 * is computed here rather than sprinkled on whichever control a wrapper
 * happens to draw first.
 */
export const FOCUS_ATTR = "data-needs-action-focus";

/**
 * Marks the whole form for the Inbox's global key handler.
 *
 * Its `j`/`k` move the row selection, and a focused Send or "I'm not sure"
 * button is not an input, so without this a person tabbing through the form
 * and typing would navigate the list behind it. Scoped to the card rather
 * than to "any button", because the Inbox's own rows are buttons and bailing
 * on those would take the list's keyboard away entirely.
 */
export const ASK_ATTR = "data-pending-ask";

/** Where the `a` shortcut should land, given what this ask is asking. */
function focusTarget(action: NeedsAction): "option" | "text" {
  return isAnswerAsk(action.expects) && askOptions(action.expects).length > 0 ? "option" : "text";
}

export function AskControls({
  action,
  inputs,
  onResolved,
  autoFocus = false,
  labels,
  extra,
}: {
  /**
   * The ask. A durable one has no gate id — it was reconstructed from the
   * run's waiting step or the proposal — so its home is what decides it.
   */
  action: NeedsAction;
  /**
   * For an adoption gate: the proposed workflow's inputs, collected beside
   * Approve because adopting *is* starting the run with them. Absent on every
   * other gate; an empty list on an adoption that takes none.
   */
  inputs?: InputDef[];
  onResolved: () => void;
  /** Take focus and answer to the Inbox's `a`. One card per screen, at most. */
  autoFocus?: boolean;
  /** The two doors' words, when the gate is about something particular — an adoption says *Adopt and start*. */
  /** The decision's verbs; a `decline` of `null` draws no Decline — a held step's release has nothing to decline. */
  labels?: { approve?: string; decline?: string | null };
  /** A third control beside the two doors — the proposal's *Request changes…*. */
  extra?: React.ReactNode;
}) {
  const toast = useToast();
  const group = useId();
  const [selected, setSelected] = useState<string[]>([]);
  const [text, setText] = useState("");
  const [values, setValues] = useState<InputValues>(() => initialValues(inputs ?? []));
  const [inputErrors, setInputErrors] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState(false);

  const question = isAnswerAsk(action.expects);
  const options = useMemo(() => askOptions(action.expects), [action.expects]);
  const multi = isMulti(action.expects);
  const anchor = focusTarget(action);

  const send = async (over: { approve?: boolean; unsure?: boolean }) => {
    if (busy) return;
    let runInputs: Record<string, unknown> | undefined;
    if (inputs && over.approve !== false) {
      const found = validateInputs(inputs, values);
      setInputErrors(found);
      if (Object.keys(found).length > 0) return;
      runInputs = toRequest(inputs, values);
    }
    const body = decideBody(action, { selected, text, inputs: runInputs, ...over });
    // `null` only when a question has neither a choice nor a sentence, which
    // is the same condition that disables Send — one function decides both,
    // so a disabled button and a silently dropped click cannot disagree.
    if (!body) return;
    setBusy(true);
    try {
      const decided = await api.decideIn(action.home, body);
      setSelected([]);
      setText("");
      // An adoption that armed a public hook minted its secret: shown once,
      // by a dialog that outlives this card — the ask is gone the moment it
      // is decided.
      showHookSecrets(decided.secrets ?? []);
      onResolved();
    } catch (e) {
      toast.error(e instanceof Error ? e.message : t("studio-pending-ask-could-not-send-answer"));
    } finally {
      setBusy(false);
    }
  };

  if (!question) {
    return (
      <div {...{ [ASK_ATTR]: "true" }} className="flex flex-col gap-2">
        {inputs && inputs.length > 0 && (
          <InputsForm inputs={inputs} values={values} errors={inputErrors} home="goal" onChange={setValues} disabled={busy} />
        )}
        <TextInput
          {...{ [FOCUS_ATTR]: autoFocus ? "true" : undefined }}
          autoFocus={autoFocus}
          value={text}
          placeholder={t("studio-pending-ask-reason-optional")}
          disabled={busy}
          onChange={(e) => setText(e.target.value)}
        />
        <div className="flex gap-2">
          <Button variant="primary" disabled={busy} onClick={() => void send({ approve: true })}>
            {busy ? "…" : (labels?.approve ?? t("studio-pending-ask-approve"))}
          </Button>
          {/* `danger`, because a decline stops work that is waiting on it. The
              Inbox drew this one as a ghost button and the goal screen as a
              danger one, which taught the same reader two things about the
              same act depending on where they met it. */}
          {labels?.decline !== null && (
            <Button variant="danger" disabled={busy} onClick={() => void send({ approve: false })}>
              {labels?.decline ?? t("studio-pending-ask-decline")}
            </Button>
          )}
          {extra}
        </div>
      </div>
    );
  }

  const ready = canSubmit(action, { selected, text });

  return (
    <div {...{ [ASK_ATTR]: "true" }} className="flex flex-col gap-2">
      {options.length > 0 && (
        <fieldset className="flex flex-col gap-0.5" disabled={busy}>
          <legend className="sr-only">
            {multi ? t("studio-pending-ask-choose-any-apply") : t("studio-pending-ask-choose-one")}
          </legend>
          {options.map((o, i) => {
            const checked = selected.includes(o.id);
            const first = i === 0;
            return (
              <label
                key={o.id}
                className={cn(
                  "anim flex cursor-pointer items-start gap-2 rounded-control px-2 py-1.5",
                  // The picked option is where you are, not another summons inside the ask: the selected ground.
                  checked ? "bg-selected text-text" : "hover:bg-surface-2",
                )}
              >
                <input
                  type={multi ? "checkbox" : "radio"}
                  // One name per card, so two questions pinned above the same
                  // conversation do not become one radio group between them.
                  name={multi ? undefined : group}
                  checked={checked}
                  {...(autoFocus && anchor === "option" && first
                    ? { [FOCUS_ATTR]: "true", autoFocus: true }
                    : {})}
                  onChange={() => setSelected((s) => toggleChoice(s, o.id, multi))}
                  onClick={() => {
                    // A radio fires no `change` when you activate the one that
                    // is already on, so unpicking a single choice has to be
                    // read from the click. `selected` is still the pre-click
                    // value here — React has not re-rendered — which is what
                    // makes the test reliable. Unpicking matters because the
                    // text box is an answer no option describes, and a stale
                    // tick riding along beside it contradicts what was typed.
                    if (!multi && selected.includes(o.id)) setSelected([]);
                  }}
                  className={cn(
                    "anim mt-0.5 h-3.5 w-3.5 shrink-0 appearance-none border border-border bg-surface",
                    "checked:border-accent checked:bg-accent",
                    multi
                      ? "rounded"
                      : "rounded-full checked:shadow-[inset_0_0_0_3px_var(--color-surface)]",
                  )}
                />
                <span className="min-w-0 flex-1">
                  <span className="flex flex-wrap items-center gap-1.5">
                    <span className="text-xs">{o.label}</span>
                    {/* Icon *and* word. The selected ground already means
                        "you picked this", so a recommended-but-unpicked option
                        cannot be marked with a fill without saying the reader
                        chose it. */}
                    {o.recommended && (
                      <Chip tone="quiet" icon={ICON.recommended}>{t("studio-pending-ask-recommended")}</Chip>
                    )}
                  </span>
                  {o.detail && (
                    <span className="mt-0.5 block text-2xs text-text-dim">{o.detail}</span>
                  )}
                </span>
              </label>
            );
          })}
        </fieldset>
      )}

      {/* Always drawn, options or not. The answers an agent thought of are not
          the answers that exist, and a question that only accepts its own
          three is a form, not a conversation. */}
      <TextArea
        {...(autoFocus && anchor === "text" ? { [FOCUS_ATTR]: "true", autoFocus: true } : {})}
        rows={options.length > 0 ? 2 : 3}
        value={text}
        placeholder={options.length > 0 ? t("studio-pending-ask-anything-options-miss") : t("studio-pending-ask-answer")}
        disabled={busy}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && (e.metaKey || e.ctrlKey) && ready) {
            e.preventDefault();
            void send({});
          }
        }}
      />

      <div className="flex flex-wrap items-center gap-2">
        <Button variant="primary" disabled={busy || !ready} onClick={() => void send({})}>
          {busy ? t("studio-pending-ask-sending") : t("studio-pending-ask-send-answer")}
        </Button>
        {/* KeyHint, not a literal ⌘: the shortcut is Ctrl+Enter off macOS, and
            a Mac glyph there names a key the keyboard does not have. */}
        <span className="flex items-center gap-1 text-2xs text-text-dim">
          <KeyHint combo="Mod+Enter" />{t("studio-pending-ask-send")}</span>
        <Button
          variant="ghost"
          className="ml-auto"
          disabled={busy}
          onClick={() => void send({ unsure: true })}
        >{t("studio-pending-ask-i-m-not-sure")}</Button>
        {/* In the row, not in a tooltip. What this button does is the one
            thing a reader needs in order to press it, and a tooltip is
            invisible on a touch screen and gone the moment the pointer moves
            — see `ui/Tooltip`. The predecessor read t("studio-pending-ask-can-t-answer") and
            declined; nothing about the words told anyone that. */}
        <p className="basis-full text-2xs text-text-dim">{t("studio-pending-ask-i-m-not-sure-answer-not")}</p>
      </div>
    </div>
  );
}
