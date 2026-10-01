/**
 * A `spawn` step's inputs: what the step gives the workflow it opens a goal
 * on. A spawn is a start by hand made by a step, so its form asks what a
 * person's start asks — every input the child declares — and what it is
 * given is a template of this run, read by the kind the child's input
 * declares. The rows, the edits and what no longer fits are the start
 * step's own rules over a mapping (`startForm.mjs`); what is here is what a
 * spawn adds: the inputs it must give and has not.
 */

import { mappingRows, setMapping, strayMappings } from "./startForm.mjs";

/** One row per input the child's workflow asks for: its name, its label, whether its run needs it, and what the step gives it. */
export function givenRows(step, asked) {
  return mappingRows(step, asked);
}

/** The step giving `template` as `input`, or giving nothing for it when the template is blank. */
export function give(step, input, template) {
  return setMapping(step, input, template);
}

/** What the step gives that the child's workflow does not ask for — shown so it can be removed, never dropped in silence. */
export function notAskedFor(step, asked) {
  return strayMappings(step, asked);
}

/** The inputs the child's run needs and the step does not give: the node refuses the design until each is given (`spawn_input`). */
export function leftOut(step, asked) {
  return givenRows(step, asked)
    .filter((row) => row.required && row.template === null)
    .map((row) => row.input);
}

/**
 * The step on another workflow: what it gave is kept where the new one asks
 * for an input of the same name, and let go where it does not — a mapping
 * for an input nobody declares is a problem the moment it is saved. Handed
 * to the Workflow Agent (`workflow` is `null`) there is nothing to give to.
 * A workflow whose inputs are not known yet (`asked` is `null`: the picker
 * named one it has not read) keeps everything: what no longer fits is shown
 * once the child is read, to be removed by hand — never dropped on a guess.
 */
export function onWorkflow(step, workflow, asked) {
  const next = { ...step, workflow: workflow ?? null };
  if (next.workflow === null) {
    delete next.inputs;
    return next;
  }
  if (asked === null || asked === undefined) return next;
  let kept = next;
  for (const name of notAskedFor(next, asked)) kept = give(kept, name, null);
  return kept;
}
