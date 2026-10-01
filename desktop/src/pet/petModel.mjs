/**
 * The pet's facts: which state it is in, how many frames each state has, and
 * where a click goes.
 *
 * Each of these is here because getting it wrong produces a wrong *fact*
 * rather than a wrong pixel — a pet that says "idle" while three agents are
 * working is worse than no pet, and a click that lands on the wrong screen is
 * worse than one that lands nowhere.
 *
 * # The format is somebody else's
 *
 * A pet package is Codex's: a `pet.json` and a 1536×1872 sheet of 192×208
 * cells, 8 columns by 9 rows, one row per state in a fixed order. That order
 * is the contract a pet was *drawn against* — row 3 is waving because the
 * artist put waving in row 3 — so `STATES` is a constant here rather than nine
 * literals spread through a renderer.
 *
 * Frame counts differ per state. A pack drawn for this platform says them
 * in its manifest — `animations`, one row per state with its frames and each
 * frame's duration — and that is what plays (`framePlan`): Jolt is quick and
 * Buffer is slow because their authors said so. A pet with no `animations`
 * (Codex's own, before openai/codex#20863) leaves its unused cells fully
 * transparent and the renderer works the counts out from the sheet
 * (`frameCounts`), one fixed duration per frame.
 *
 * Nine pets ship with the platform; **Moonrice** is the one shown when the
 * pet is turned on and none was chosen (`DEFAULT_PET_ID`, `defaultPet`).
 */

import { stateOf } from "../ui/sessionState.mjs";
import { t } from "../i18n/l10n.mjs";

/** The nine rows, in the order the sheet lays them out. */
export const STATES = [
  "idle",
  "running-right",
  "running-left",
  "waving",
  "jumping",
  "failed",
  "waiting",
  "running",
  "review",
];

/** The atlas geometry the format defines. */
export const SHEET = {
  width: 1536,
  height: 1872,
  cols: 8,
  rows: 9,
  cellWidth: 192,
  cellHeight: 208,
};

/**
 * Milliseconds a frame is held, and how much slower idle runs.
 *
 * Taken from the defaults proposed in openai/codex#20863 (`durationMs: 150`,
 * `idleSlowdown: 6`) rather than invented, so a pet drawn for Codex moves here
 * at the pace its author was watching when they drew it.
 */
export const FRAME_MS = 150;
export const IDLE_SLOWDOWN = 6;

/**
 * The state the pet rests in, given what is true right now.
 *
 * Ordered by what it would cost you to miss. Something waiting on you outranks
 * everything, because it is the only one of these that stops progress; a
 * finished thing to look at outranks work in progress, because the work will
 * still be there; and idle is what is left.
 */
export function standingState({ waiting = 0, review = 0, working = 0 } = {}) {
  if (waiting > 0) return "waiting";
  if (review > 0) return "review";
  if (working > 0) return "running";
  return "idle";
}

/**
 * The standing state while the pet follows one harness session: the
 * session's own word, in the pet's rows. Working is working whichever
 * phase it is in; *failed* and *aborted* stand — the harness is failed
 * until it is gone or resumed — and *done* rests, the jump having played
 * once on the way in (`transientForTransition`).
 * @param {object | string} state a roster session's state
 */
export function petStateOfSession(state) {
  switch (stateOf(state)) {
    case "starting":
    case "thinking":
    case "running":
      return "running";
    case "waiting":
      return "waiting";
    case "failed":
    case "aborted":
      return "failed";
    default:
      return "idle";
  }
}

/**
 * The once-through state a followed session's edge earns: a finish jumps,
 * a failure or an abort plays failed, everything else is the standing
 * state's business. Only edges — a state repeated is not news.
 * @param {object | string | null} prev
 * @param {object | string} next
 */
export function transientForTransition(prev, next) {
  const before = prev === null || prev === undefined ? null : stateOf(prev);
  const after = stateOf(next);
  if (before === after) return null;
  if (after === "done") return "jumping";
  if (after === "failed" || after === "aborted") return "failed";
  return null;
}

/**
 * Which engine event, if any, the pet should react to by playing something
 * once.
 *
 * Only events that are *about the work* qualify. A pet that jumped every time
 * a token arrived would be a pet nobody could look at, which is why the
 * streaming payloads are absent from this list rather than merely unhandled.
 */
export function transientFor(payload) {
  const type = typeof payload === "string" ? payload : payload?.type;
  switch (type) {
    // A start event that could not start its run: the failure nobody saw.
    case "listener_failed":
      return "failed";
    // A step is what fails now; the payload says which way it moved, and only
    // a failure earns the reaction. A step finishing is ordinary progress.
    case "step_changed":
      return typeof payload === "object" && payload?.state === "failed" ? "failed" : null;
    case "run_finished":
      return typeof payload === "object" && payload?.outcome === "failed" ? "failed" : "jumping";
    // A session's end is typed: only a failure is news.
    case "execution_ended":
      return typeof payload === "object" && payload?.outcome?.outcome === "failed" ? "failed" : null;
    case "gate_decided":
    case "result_accepted":
      return "jumping";
    // The Workflow Agent's standing: a proposal is good news, a stall or a
    // failure is not; the rest is the platform at work, which the standing
    // state already shows.
    case "guided": {
      const status = typeof payload === "object" ? payload?.status : null;
      if (status === "proposed") return "jumping";
      if (status === "stalled" || status === "failed") return "failed";
      return null;
    }
    default:
      return null;
  }
}

/**
 * The direction a drag is heading, as a state.
 *
 * The one pair of rows that only means something for a pet you can pick up:
 * `running-right` and `running-left` exist so the character faces the way it
 * is travelling. A drag that has not really moved returns `null` so the pet
 * keeps whatever it was doing rather than flickering between the two.
 */
export function dragState(dx, threshold = 2) {
  if (dx > threshold) return "running-right";
  if (dx < -threshold) return "running-left";
  return null;
}

/**
 * How many frames each row actually uses.
 *
 * `alphaAt(row, col)` answers whether a cell has any non-transparent pixel.
 * Frames run from column 0 rightwards and the unused tail is transparent, so
 * the count is one past the last cell with anything in it.
 *
 * **A row with nothing in it counts as one frame, not zero.** A pet whose
 * artist skipped a state should stand still on an empty cell, not divide the
 * clock by zero — and the caller should not have to remember that.
 */
export function frameCounts(alphaAt, sheet = SHEET) {
  const counts = [];
  for (let row = 0; row < sheet.rows; row++) {
    let last = -1;
    for (let col = 0; col < sheet.cols; col++) {
      if (alphaAt(row, col)) last = col;
    }
    counts.push(last + 1 || 1);
  }
  return counts;
}

/**
 * Read a sheet's pixels into `alphaAt`.
 *
 * Split from `frameCounts` so the decision is testable and only the pixel
 * arithmetic touches image data. `data` is RGBA, four bytes per pixel, row
 * major — what `CanvasRenderingContext2D.getImageData` returns.
 *
 * Every fourth byte is sampled on a coarse grid rather than every pixel: a
 * frame either has art in it or is blank, so reading three million alpha
 * values to answer nine questions is work nobody needs done.
 */
export function alphaProbe(data, sheet = SHEET, step = 8) {
  return (row, col) => {
    const x0 = col * sheet.cellWidth;
    const y0 = row * sheet.cellHeight;
    for (let y = y0; y < y0 + sheet.cellHeight; y += step) {
      for (let x = x0; x < x0 + sheet.cellWidth; x += step) {
        if (data[(y * sheet.width + x) * 4 + 3] > 8) return true;
      }
    }
    return false;
  };
}

/** How tall the pet stands on screen at 100%, whatever its sheet's resolution. */
export const PET_HEIGHT = 104;

/**
 * How far the size dial travels, as a percentage of [`PET_HEIGHT`].
 *
 * Percent rather than pixels because that is what the reader is choosing —
 * "half again as big", not "156 pixels tall" — and because storing the percent
 * keeps the number stable if the base height is ever retuned.
 *
 * The ceiling is deliberately low. A pet is a companion that floats over the
 * work, and one scaled past about half again is no longer glanceable decoration
 * but an obstruction sitting on top of the thing you are reading — with a hit
 * area to match, since the whole sprite is its own drag handle. The floor is
 * where the nine states stop being distinguishable at a glance, which is the
 * only reason to have a pet at all.
 */
export const PET_SIZE_MIN = 50;
export const PET_SIZE_MAX = 150;
export const PET_SIZE_STEP = 10;
export const PET_SIZE_DEFAULT = 100;

/**
 * A stored or typed size, made safe to render at.
 *
 * Anything unusable — absent, `NaN`, a string, negative, absurd — becomes the
 * default rather than an error. This reads `localStorage`, which a person can
 * edit and a previous version can have written, and a pet that refuses to draw
 * because its size is unparseable is a worse outcome than one at 100%.
 */
/**
 * Numeric input from storage or a control, or `NaN` when there is no number in
 * it at all.
 *
 * `Number()` alone is not enough and the difference is not academic:
 * `Number(null)`, `Number("")` and `Number([])` are all **0**, so an absent
 * preference would clamp to the *minimum* rather than fall back to the
 * default — the smallest legal value, silently, for anyone whose storage was
 * empty. Only a real number or a string with something in it counts.
 */
function numeric(raw) {
  if (typeof raw === "number") return raw;
  if (typeof raw === "string" && raw.trim() !== "") return Number(raw);
  return NaN;
}

export function clampPetSize(size) {
  const n = numeric(size);
  if (!Number.isFinite(n)) return PET_SIZE_DEFAULT;
  return Math.min(Math.max(Math.round(n), PET_SIZE_MIN), PET_SIZE_MAX);
}

/** The height in pixels a size percentage asks for. */
export function petHeight(size) {
  return Math.round((PET_HEIGHT * clampPetSize(size)) / 100);
}

/**
 * The box one cell occupies on screen, and the sheet size behind it.
 *
 * **The grid is the contract; the pixel dimensions are not.** A pet is 8×9
 * cells — that is what an artist draws against and what the row order means.
 * The reference sheet is 1536×1872, but a pet exported at any other resolution
 * is still a valid pet, and hard-coding the reference size cropped every one of
 * them to the wrong rectangle: the window stayed 192×208 while the art behind
 * it was a different size, so each frame showed a slice of two.
 *
 * So the cell is measured — natural size divided by the grid — and everything
 * is scaled to put the pet at a consistent height on screen. Two pets drawn at
 * different resolutions then stand the same height instead of one towering
 * over the other.
 *
 * A sheet whose size is not known yet falls back to the reference, so the first
 * paint is right for the common case and corrected for the rest.
 */
export function spriteBox(naturalWidth, naturalHeight, targetHeight = PET_HEIGHT, sheet = SHEET) {
  const cellW = naturalWidth > 0 ? naturalWidth / sheet.cols : sheet.cellWidth;
  const cellH = naturalHeight > 0 ? naturalHeight / sheet.rows : sheet.cellHeight;
  const scale = targetHeight / cellH;
  const width = Math.round(cellW * scale);
  const height = Math.round(cellH * scale);
  return {
    width,
    height,
    sheetWidth: width * sheet.cols,
    sheetHeight: height * sheet.rows,
  };
}

/**
 * Where to move the background so cell `(row, frame)` is the one showing.
 *
 * Negative offsets, because the sheet moves under a window rather than the
 * window moving over the sheet. The step is the *rendered* cell from
 * [`spriteBox`] rather than the spec's, so a sheet at any resolution lands on
 * whole frames.
 */
export function cellOffset(row, frame, box) {
  // `|| 0` normalises negative zero. `-0` is a valid CSS length and animates
  // identically, but it compares unequal to `0` and would make every caller
  // that checks "has this moved" wrong once per loop.
  return {
    x: -frame * box.width || 0,
    y: -row * box.height || 0,
  };
}

/** How long one frame of a state is held, for a pet whose manifest says nothing. */
export function frameDuration(state) {
  return state === "idle" ? FRAME_MS * IDLE_SLOWDOWN : FRAME_MS;
}

/**
 * How a state plays: its frame count and each frame's duration in
 * milliseconds. The manifest's word when it animates the state — a
 * `frames` count and, when given, one duration per frame, else the pack's
 * `fps` — and otherwise the probed count with the fixed pace. A count the
 * manifest gives past the sheet's columns is held to them; a plan always
 * has at least one frame.
 * @param {{animations?: Record<string, {row: number, frames: number, frameDurationsMs?: number[]}>, fps?: number} | null | undefined} def
 * @param {string} state
 * @param {number} probed the frames the sheet's alpha showed for the state's row
 * @returns {{frames: number, durations: number[], fromManifest: boolean}}
 */
export function framePlan(def, state, probed) {
  const anim = def?.animations?.[state];
  if (anim && Number.isFinite(anim.frames) && anim.frames >= 1) {
    const frames = Math.min(Math.floor(anim.frames), SHEET.cols);
    const given = Array.isArray(anim.frameDurationsMs) ? anim.frameDurationsMs : [];
    const pace = def?.fps && def.fps > 0 ? Math.round(1000 / def.fps) : FRAME_MS;
    const durations = Array.from({ length: frames }, (_, i) => {
      const d = given[i];
      return Number.isFinite(d) && d > 0 ? Math.round(d) : pace;
    });
    return { frames, durations, fromManifest: true };
  }
  const frames = Math.max(1, Math.floor(probed) || 1);
  return { frames, durations: Array.from({ length: frames }, () => frameDuration(state)), fromManifest: false };
}

/** The pet shown when the pet is turned on and none was chosen. */
export const DEFAULT_PET_ID = "bisa-pets.midnight-shipping.moonrice";

/**
 * Which pet to show when one is asked for: the last one shown, while it
 * still exists; else Moonrice, while it ships; else the first built-in; else
 * the first pet at all; else none.
 * @param {readonly {id: string, origin?: string}[]} pets
 * @param {string | null | undefined} last
 * @returns {string | null}
 */
export function defaultPet(pets, last) {
  const has = (id) => pets.some((p) => p.id === id);
  if (last && has(last)) return last;
  if (has(DEFAULT_PET_ID)) return DEFAULT_PET_ID;
  return pets.find((p) => p.origin === "catalog")?.id ?? pets[0]?.id ?? null;
}

/**
 * What the pet is doing, in the catalog's words — the half of its label a
 * person reads it for: *waiting on you*, *working*, *something to review*.
 * @param {string} state one of `STATES`
 */
export function stateWords(state) {
  return t("pet-state", { state });
}

/** Where a pet comes from, in a word for its tile. */
export function originWords(origin) {
  return origin === "catalog" ? t("pet-pet-built") : t("pet-pet-yours");
}

/**
 * The words a tile carries under the name: the pack's tagline, and the
 * archetype and mood as chips — what the manifest says, nothing invented.
 * @param {{description?: string, "x-bisa-pets"?: {tagline?: string, archetype?: string, mood?: string}}} def
 * @returns {{tagline: string, chips: string[]}}
 */
export function tileWords(def) {
  const flavour = def?.["x-bisa-pets"] ?? {};
  const tagline = (flavour.tagline ?? "").trim() || firstSentence(def?.description ?? "");
  const chips = [flavour.archetype, flavour.mood].map((w) => (w ?? "").trim()).filter(Boolean);
  return { tagline, chips };
}

function firstSentence(text) {
  const m = /^(.*?[.!?])(\s|$)/.exec(text.trim());
  return m ? m[1] : text.trim();
}

/**
 * Where clicking the pet should take you.
 *
 * The pet is reacting to something; a click should go to that something. When
 * it is idle it is reacting to nothing, and the honest answer is that nothing
 * happens — better than picking a screen and pretending it was meant.
 *
 * `scope` is the conversation the working agent is in, which is a goal id
 * whenever the work is a goal's.
 */
export function routeFor(state, scope, follow = null) {
  if (follow) {
    // Following a harness, the pet is a door to it: the workstream's Agents
    // panel, whatever it is saying — except nothing, which stays nothing.
    return state === "waiting" || state === "review" || state === "failed" || state === "running"
      ? { name: "workbench", scope: "workstream", id: follow.workstream }
      : null;
  }
  switch (state) {
    case "waiting":
      return { name: "inbox" };
    case "review":
      return { name: "inbox" };
    case "failed":
    case "running":
      return scope ? { name: "goal", id: scope } : { name: "goals" };
    default:
      return null;
  }
}

/** The row a state occupies, or 0 for a name the sheet does not have. */
export function rowOf(state) {
  const i = STATES.indexOf(state);
  return i < 0 ? 0 : i;
}
