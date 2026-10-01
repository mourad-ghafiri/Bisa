/**
 * Whether a body fits the schema's definition of its type — the validator
 * the guard `bodiesFitTheSchema.test.mjs` holds the desktop's bodies to,
 * and that test's own helper. The node refuses a body carrying a key its type does not
 * declare, TypeScript checks a fresh literal only, and no test runs the
 * window: a body is checked here, against `api-schema.json`, the file the
 * types are generated from.
 *
 * Written by hand, for what that file uses and nothing more: `$ref` into
 * `definitions`, `type` (a word or a list), `properties`, `required`,
 * `additionalProperties`, `patternProperties`, `items` (one shape, or a
 * tuple's), `minItems` / `maxItems`, `enum`, `const`, `anyOf`, `oneOf`,
 * `allOf`, `minimum` / `maximum`, `pattern`, and the integer `format`s.
 * `default` and `description` say nothing about a value.
 *
 * Not a model: it has no `.d.mts` because nothing in the app imports it,
 * only the tests do — as `testWalk.mjs`. Its words are a developer's, read
 * in a failed test.
 */

/** The bounds an integer `format` holds a number to. */
const INTEGER_FORMATS = Object.freeze({
  uint8: [0, 255],
  uint16: [0, 65535],
  uint32: [0, 4294967295],
  uint: [0, Number.MAX_SAFE_INTEGER],
  uint64: [0, Number.MAX_SAFE_INTEGER],
  int8: [-128, 127],
  int16: [-32768, 32767],
  int32: [-2147483648, 2147483647],
  int64: [Number.MIN_SAFE_INTEGER, Number.MAX_SAFE_INTEGER],
});

const isRecord = (v) => v !== null && typeof v === "object" && !Array.isArray(v);

/** The word JSON has for a value's type; `integer` is a `number` that is whole. */
function typeOf(v) {
  if (v === null) return "null";
  if (Array.isArray(v)) return "array";
  if (typeof v === "number") return Number.isInteger(v) ? "integer" : "number";
  return typeof v;
}

/** Whether a value is of a type the schema names. */
function isOfType(v, type) {
  const is = typeOf(v);
  return is === type || (type === "number" && is === "integer");
}

/** Two JSON values, equal by content. */
function same(a, b) {
  return JSON.stringify(a) === JSON.stringify(b);
}

/** The definitions of a schema file, by name. @param {object} schema */
export function definitionsOf(schema) {
  return schema?.definitions ?? schema?.$defs ?? {};
}

/** The definition a `$ref` names, and its name. */
function resolve(ref, defs) {
  const name = String(ref).replace(/^#\/(definitions|\$defs)\//, "");
  return { name, node: defs[name] };
}

/**
 * Every way `value` does not fit `node`, each as `where: why` — empty when
 * it fits.
 * @param {unknown} node a schema: an object, or `true` / `false`
 * @param {unknown} value what is checked — as `JSON.stringify` would send it
 * @param {{defs: Record<string, unknown>, closed: ReadonlySet<string>}} ctx
 * @param {string} at where in the body
 * @param {string | null} [named] the definition `node` is, when it is one
 * @returns {string[]}
 */
function misfitsAt(node, value, ctx, at, named = null) {
  if (node === true || node === undefined) return [];
  if (node === false) return [`${at}: nothing is taken here`];
  if (!isRecord(node)) return [`${at}: the schema here is no schema`];
  if (typeof node.$ref === "string") {
    const { name, node: target } = resolve(node.$ref, ctx.defs);
    if (target === undefined) return [`${at}: the schema names \`${name}\`, which it does not define`];
    return misfitsAt(target, value, ctx, at, name);
  }

  const out = [];
  if (node.type !== undefined) {
    const types = Array.isArray(node.type) ? node.type : [node.type];
    if (!types.some((t) => isOfType(value, t))) return [`${at}: ${typeOf(value)} where ${types.join(" or ")} is taken`];
  }
  if ("const" in node && !same(node.const, value)) out.push(`${at}: ${JSON.stringify(value)} where only ${JSON.stringify(node.const)} is taken`);
  if (Array.isArray(node.enum) && !node.enum.some((e) => same(e, value))) out.push(`${at}: ${JSON.stringify(value)} is none of ${node.enum.map((e) => JSON.stringify(e)).join(", ")}`);

  if (typeof value === "number") {
    if (typeof node.minimum === "number" && value < node.minimum) out.push(`${at}: ${value} is under ${node.minimum}`);
    if (typeof node.maximum === "number" && value > node.maximum) out.push(`${at}: ${value} is over ${node.maximum}`);
    const bounds = INTEGER_FORMATS[node.format];
    if (bounds && (!Number.isInteger(value) || value < bounds[0] || value > bounds[1])) out.push(`${at}: ${value} is no ${node.format}`);
  }
  if (typeof value === "string" && typeof node.pattern === "string" && !new RegExp(node.pattern).test(value)) out.push(`${at}: ${JSON.stringify(value)} does not read as ${node.pattern}`);

  if (Array.isArray(value)) {
    if (typeof node.minItems === "number" && value.length < node.minItems) out.push(`${at}: ${value.length} items where at least ${node.minItems} are taken`);
    if (typeof node.maxItems === "number" && value.length > node.maxItems) out.push(`${at}: ${value.length} items where at most ${node.maxItems} are taken`);
    if (Array.isArray(node.items)) value.forEach((item, i) => out.push(...misfitsAt(node.items[i] ?? node.additionalItems, item, ctx, `${at}[${i}]`)));
    else if (node.items !== undefined) value.forEach((item, i) => out.push(...misfitsAt(node.items, item, ctx, `${at}[${i}]`)));
  }

  for (const part of node.allOf ?? []) out.push(...misfitsAt(part, value, ctx, at));

  // A value fits a choice when it fits one of its shapes. Serde picks a
  // shape by its tag, so two never fit at once where it matters; the shapes
  // that fit are kept, since a closed shape knows their keys too.
  const fitting = [];
  for (const key of ["anyOf", "oneOf"]) {
    const shapes = node[key];
    if (!Array.isArray(shapes)) continue;
    const tried = shapes.map((shape) => ({ shape, problems: misfitsAt(shape, value, ctx, at) }));
    const fits = tried.filter((t) => t.problems.length === 0);
    if (fits.length > 0) {
      fitting.push(...fits.map((t) => t.shape));
      continue;
    }
    // None fits: the nearest shape says why — the one whose tag is this
    // value's, else the one with the fewest problems — and its keys are
    // known, so a key of the kind the value is never reads as a stranger.
    const tagged = tried.filter((t) => !t.problems.some((p) => p.includes("where only")));
    const nearest = (tagged.length > 0 ? tagged : tried).sort((a, b) => a.problems.length - b.problems.length)[0];
    out.push(`${at}: fits none of the ${shapes.length} shapes taken here`, ...(nearest?.problems ?? []));
    if (nearest) fitting.push(nearest.shape);
  }

  if (isRecord(value)) {
    const properties = isRecord(node.properties) ? node.properties : {};
    for (const key of node.required ?? []) if (!(key in value) || value[key] === undefined) out.push(`${at}: \`${key}\` is missing`);
    const patterns = Object.entries(isRecord(node.patternProperties) ? node.patternProperties : {});
    // What a closed shape knows beside its own keys: the keys of the shapes of its choice that fit.
    const known = new Set(Object.keys(properties));
    const closed = named !== null && ctx.closed.has(named);
    if (closed) for (const shape of fitting) for (const key of Object.keys(shapeProperties(shape, ctx.defs))) known.add(key);
    for (const [key, held] of Object.entries(value)) {
      // What `JSON.stringify` leaves out never reaches the node.
      if (held === undefined) continue;
      const where = `${at}.${key}`;
      if (key in properties) {
        out.push(...misfitsAt(properties[key], held, ctx, where));
        continue;
      }
      const matched = patterns.filter(([pattern]) => new RegExp(pattern).test(key));
      if (matched.length > 0) {
        for (const [, shape] of matched) out.push(...misfitsAt(shape, held, ctx, where));
        continue;
      }
      if (known.has(key)) continue;
      if (node.additionalProperties === false || closed) out.push(`${where}: a key ${named ? `\`${named}\`` : "the type"} does not declare`);
      else if (isRecord(node.additionalProperties)) out.push(...misfitsAt(node.additionalProperties, held, ctx, where));
    }
  }
  return out;
}

/** The keys one shape of a choice declares, through a `$ref` when it is one. */
function shapeProperties(shape, defs) {
  if (!isRecord(shape)) return {};
  if (typeof shape.$ref === "string") return shapeProperties(resolve(shape.$ref, defs).node, defs);
  return isRecord(shape.properties) ? shape.properties : {};
}

// Read by `bodiesFitTheSchema.test.mjs` and the scenario suites (`goals`, `files`, `gitPanel`, `workflows`, `workstreams`): the check every body is held to.
/**
 * Every way `body` does not fit the schema's definition `name` — empty when
 * it fits. The body is read as the wire would carry it: through
 * `JSON.stringify`, so a key left `undefined` is a key that is absent.
 *
 * `closed` names the definitions the node holds to their keys **by hand** —
 * a shape serde flattens cannot refuse a key it does not know, so the core
 * checks those itself and the schema cannot say so: such a definition knows
 * its own keys and the keys of the shape of its choice that fits, and
 * nothing else.
 * @param {object} schema the schema file, parsed
 * @param {string} name a definition's name
 * @param {unknown} body
 * @param {{closed?: Iterable<string>}} [options]
 * @returns {string[]}
 */
export function misfits(schema, name, body, options = {}) {
  const defs = definitionsOf(schema);
  if (!(name in defs)) return [`the schema defines no \`${name}\``];
  const sent = body === undefined ? undefined : JSON.parse(JSON.stringify(body));
  return misfitsAt(defs[name], sent, { defs, closed: new Set(options.closed ?? []) }, "$", name);
}

// Read by `bodiesFitTheSchema.test.mjs`: which types the node holds to their keys, so the hand-held list there stays exact.
/**
 * Whether the schema holds a definition to its keys: every shape it may be
 * says `additionalProperties: false`.
 * @param {object} schema @param {string} name
 */
export function refusesUnknownKeys(schema, name) {
  const node = definitionsOf(schema)[name];
  if (!isRecord(node)) return false;
  const shapes = node.oneOf ?? node.anyOf;
  if (Array.isArray(shapes) && !isRecord(node.properties)) return shapes.every((shape) => isRecord(shape) && shape.additionalProperties === false);
  return node.additionalProperties === false;
}
