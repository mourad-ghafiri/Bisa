/**
 * The role contract, translated for xterm.
 *
 * Plain `.mjs` with a `.d.mts` beside it, rather than TypeScript, so
 * `node --test` can import the real module instead of a copy of it. The theme
 * contract is already tested this way (`theme/roles.test.mjs`), and for the
 * same reason: a mapping that is asserted against a *transcription* of itself
 * is not asserted at all.
 *
 * # Two halves, and only one of them is ours
 *
 * The **chrome** — what the panel is made of — belongs to the theme. Terminal
 * background is `--color-surface` because the terminal sits where a card sits;
 * the cursor is `--color-accent` because the accent means "your attention" and
 * a caret waiting for you to type is the most literal case of that in the app.
 * Pick Dune, pick compact, pick a teal accent: the terminal follows, because
 * it never names a colour.
 *
 * The **ANSI ramp** does not, and making it follow would be a bug dressed as
 * consistency. ANSI 1 does not mean "danger", it means *the program said red*
 * — `git diff` says it about a deleted line, `ls` says it about a broken
 * symlink, a compiler says it about an error. Wiring those onto
 * `--color-danger` would make `git diff` change hue when someone switches
 * accent, and would collapse red and magenta into one colour in any theme
 * whose danger and accent are close. Sixteen colours the terminal owns
 * outright is the honest answer.
 *
 * What the ramp does follow is the *side* the mounted theme landed on, because
 * a ramp tuned for near-white is unreadable on near-black and vice versa. Two
 * sets, chosen by `data-scheme` — the same authority the accent overlays use.
 */

/**
 * The token roles this module reads.
 *
 * Exported so the test can check them against `tokens.css`. A role the
 * terminal asks for that no theme supplies does not look wrong, it resolves to
 * nothing — which for `background` means a transparent terminal drawn over
 * whatever is behind it, and for `foreground` means invisible output.
 */
export const TERMINAL_ROLES = [
  "--color-surface",
  "--color-surface-2",
  "--color-text",
  "--color-accent",
  "--color-accent-soft",
];

/**
 * The last-resort chrome, per side.
 *
 * Not a theme — the same role a flat grey app plays in `tokens.css`. If these
 * are what you are looking at, no theme is mounted, and a terminal that is
 * merely plain is a far louder failure than one that is invisible.
 */
const FALLBACK = {
  light: {
    background: "#fdfdfe",
    foreground: "#24272e",
    selection: "#e9e2d3",
    inactiveSelection: "#eceef1",
  },
  dark: {
    background: "#16161a",
    foreground: "#e4e4e9",
    selection: "#4a3f2c",
    inactiveSelection: "#30343d",
  },
};

/**
 * The ramps: two original sets, drawn as pairs on the surfaces the
 * families actually use — the light one on an L 0.995 card, the dark one on
 * an L 0.235 card (Suede's paper and Glass's frost are hexed to a solid near
 * those) — so every colour clears 3:1 against its side and no
 * two are within 25° of one another. The light ramp is deep and a little
 * desaturated, because a saturated yellow on near-white is the classic
 * unreadable terminal; the dark ramp is lifted and softened, because a pure
 * red on near-black vibrates.
 *
 * Bright is the same hue as normal, one step lighter, for everything except
 * black and white: a program using bright red means emphasis, not a different
 * red, and a ramp where the two differ in hue makes `ls` look like a warning.
 */
const ANSI = {
  light: {
    black: "#3b3f4a",
    red: "#c22e3a",
    green: "#2f8a3d",
    yellow: "#a86b00",
    blue: "#2461c9",
    magenta: "#a63fb5",
    cyan: "#0f8a97",
    white: "#a8adb8",
    brightBlack: "#626878",
    brightRed: "#d6414d",
    brightGreen: "#3a9e4a",
    brightYellow: "#bf7f0a",
    brightBlue: "#3b74d9",
    brightMagenta: "#b955c7",
    brightCyan: "#1a9ea9",
    brightWhite: "#c3c7d1",
  },
  dark: {
    black: "#4a4f5c",
    red: "#f08a94",
    green: "#8fd19a",
    yellow: "#e6c37a",
    blue: "#86b3f5",
    magenta: "#d9a0e6",
    cyan: "#7fd0d6",
    white: "#b9bfcc",
    brightBlack: "#6b7180",
    brightRed: "#f79ca5",
    brightGreen: "#a3dcad",
    brightYellow: "#eed08f",
    brightBlue: "#9dc2f8",
    brightMagenta: "#e3b2ec",
    brightCyan: "#97dbe0",
    brightWhite: "#d5dae4",
  },
};

/**
 * An xterm `ITheme` from resolved token values.
 *
 * `resolved` maps a role to a colour xterm can parse — `#rrggbb`, not the
 * `oklch(...)` a custom property actually holds. Resolving is the caller's job
 * because it needs a DOM; deciding what each role *means* is this function's,
 * because that is the part worth testing.
 *
 * An empty or missing value falls back rather than being passed through: xterm
 * throws on a colour it cannot parse, and a terminal that refuses to construct
 * is a blank panel with a console error nobody sees.
 */
export function xtermTheme(resolved, scheme) {
  const side = scheme === "dark" ? "dark" : "light";
  const fallback = FALLBACK[side];
  const pick = (role, spare) => {
    const value = typeof resolved?.[role] === "string" ? resolved[role].trim() : "";
    return value || spare;
  };

  const background = pick("--color-surface", fallback.background);
  const foreground = pick("--color-text", fallback.foreground);

  return {
    background,
    foreground,
    cursor: pick("--color-accent", foreground),
    // A block cursor paints the character under it in this colour, so it has
    // to be the background or the character disappears inside the caret.
    cursorAccent: background,
    selectionBackground: pick("--color-accent-soft", fallback.selection),
    selectionInactiveBackground: pick("--color-surface-2", fallback.inactiveSelection),
    ...ANSI[side],
  };
}
