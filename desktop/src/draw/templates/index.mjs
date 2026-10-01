/**
 * The templates *New drawing* offers (19 — Drawings): *Empty*, then ten
 * pictures a workspace reaches for — each a skeleton the canvas lays out, in
 * the words of `grid.mjs`, with a label, a one-line **blurb** saying when to
 * reach for it, and a **preview** drawn from the skeleton itself
 * (`preview.mjs`). Generic by design: the platform ships no scenario's
 * diagram, only the shapes of pictures anyone draws. Every word a template
 * draws comes through the catalog — none is written here in English.
 *
 * Plain `.mjs`, so `node --test` reads it.
 */

import { t } from "../../i18n/l10n.mjs";
import { PALETTE, arrow, box, caption, col, frame, note, row, title } from "./grid.mjs";

/**
 * The words the templates are drawn with — for a person, so through the
 * catalog: a template is drawn in the window's language, and what it says is
 * then the person's to edit like any stroke.
 */
const W = Object.freeze({
  systemArchitecture: t("draw-tpl-system-architecture"),
  webClient: t("draw-tpl-web-client"),
  apiGateway: t("draw-tpl-api-gateway"),
  ordersService: t("draw-tpl-orders-service"),
  usersService: t("draw-tpl-users-service"),
  ordersDb: t("draw-tpl-orders-db"),
  doTheThing: t("draw-tpl-do-the-thing"),
  fixIt: t("draw-tpl-fix-it"),
  theIdea: t("draw-tpl-the-idea"),
  mindMap: t("draw-tpl-mind-map"),
  toDo: t("draw-tpl-to-do"),
  userJourney: t("draw-tpl-user-journey"),
  whatTheyFeel: t("draw-tpl-what-they-feel"),
  entityCustomer: t("draw-tpl-entity-customer"),
  entityOrder: t("draw-tpl-entity-order"),
  entityOrderLine: t("draw-tpl-entity-order-line"),
  entityProduct: t("draw-tpl-entity-product"),
  wentWell: t("draw-tpl-went-well"),
  couldBeBetter: t("draw-tpl-could-be-better"),
  tryNext: t("draw-tpl-try-next"),
  bodyCopy: t("draw-tpl-body-copy"),
  primaryAction: t("draw-tpl-primary-action"),
  whatThisScreenIsFor: t("draw-tpl-what-this-screen-is-for"),
  discover: t("draw-tpl-discover"),
  signUp: t("draw-tpl-sign-up"),
  firstUse: t("draw-tpl-first-use"),
  return: t("draw-tpl-return"),
  recommend: t("draw-tpl-recommend"),
  cache: t("draw-tpl-cache"),
  backend: t("draw-tpl-backend"),
  start: t("draw-tpl-start"),
  worked: t("draw-tpl-worked"),
  done: t("draw-tpl-done"),
  doing: t("draw-tpl-doing"),
  yes: t("draw-tpl-yes"),
  no: t("draw-tpl-no"),
  why: t("draw-tpl-why"),
  what: t("draw-tpl-what"),
  who: t("draw-tpl-who"),
  when: t("draw-tpl-when"),
  customer: t("draw-tpl-customer"),
  support: t("draw-tpl-support"),
  engineering: t("draw-tpl-engineering"),
  screenTitle: t("draw-tpl-screen-title"),
  image: t("draw-tpl-image"),
  headline: t("draw-tpl-headline"),
  kickOff: t("draw-tpl-kick-off"),
  alpha: t("draw-tpl-alpha"),
  beta: t("draw-tpl-beta"),
  launch: t("draw-tpl-launch"),
  review: t("draw-tpl-review"),
});

/** The template labels, once: the title drawn at the top of a picture is its template's name. */
const LABEL = Object.freeze({
  empty: t("draw-template-empty"),
  systemArchitecture: t("draw-template-system-architecture"),
  flowchart: t("draw-template-flowchart"),
  swimlanes: t("draw-template-swimlanes"),
  mindMap: t("draw-template-mind-map"),
  kanban: t("draw-template-kanban"),
  userJourney: t("draw-template-user-journey"),
  entityRelationship: t("draw-template-entity-relationship"),
  retrospective: t("draw-template-retrospective"),
  wireframe: t("draw-template-wireframe"),
  timeline: t("draw-template-timeline"),
});

/** Empty: a blank canvas. */
function empty() {
  return [];
}

/** A system's parts and how they talk: client → gateway → services → stores. */
function systemArchitecture() {
  const client = box("client", col(0), row(1), W.webClient, { fill: PALETTE.blue });
  const gateway = box("gateway", col(1), row(1), W.apiGateway, { fill: PALETTE.yellow });
  const orders = box("svc-orders", col(2), row(0), W.ordersService, { fill: PALETTE.green });
  const users = box("svc-users", col(2), row(2), W.usersService, { fill: PALETTE.green });
  const db = box("db", col(3), row(0), W.ordersDb, { type: "ellipse", fill: PALETTE.grey });
  const cache = box("cache", col(3), row(2), W.cache, { type: "ellipse", fill: PALETTE.grey });
  return [
    title("title", col(0), 20, W.systemArchitecture),
    client,
    gateway,
    orders,
    users,
    db,
    cache,
    arrow("a1", client, gateway, "TLS"),
    arrow("a2", gateway, orders),
    arrow("a3", gateway, users),
    arrow("a4", orders, db),
    arrow("a5", users, cache),
    frame("backend", W.backend, ["gateway", "svc-orders", "svc-users", "db", "cache"]),
  ];
}

/** A flow with a decision. */
function flowchart() {
  const start = box("start", col(0), row(1), W.start, { type: "ellipse", fill: PALETTE.green });
  const step1 = box("step1", col(1), row(1), W.doTheThing);
  const check = box("check", col(2), row(1), W.worked, { type: "diamond", width: 200, height: 120, fill: PALETTE.yellow });
  const yes = box("done", col(3), row(0), W.done, { type: "ellipse", fill: PALETTE.green });
  const no = box("fix", col(3), row(2), W.fixIt, { fill: PALETTE.red });
  return [
    title("title", col(0), 20, LABEL.flowchart),
    start,
    step1,
    check,
    yes,
    no,
    arrow("a1", start, step1),
    arrow("a2", step1, check),
    arrow("a3", check, yes, W.yes),
    arrow("a4", check, no, W.no),
    arrow("a5", no, step1),
  ];
}

/** Lanes, one per actor, with the steps in each. */
function swimlanes() {
  const lanes = [W.customer, W.support, W.engineering];
  const out = [title("title", col(0), 20, LABEL.swimlanes)];
  const ids = [];
  lanes.forEach((lane, i) => {
    const label = caption(`lane-${i}`, col(0), row(i) + 20, lane);
    const a = box(`s-${i}-a`, col(1), row(i), t("draw-tpl-lane-step", { lane, n: 1 }), { fill: [PALETTE.blue, PALETTE.yellow, PALETTE.green][i] });
    const b = box(`s-${i}-b`, col(2), row(i), t("draw-tpl-lane-step", { lane, n: 2 }), { fill: [PALETTE.blue, PALETTE.yellow, PALETTE.green][i] });
    out.push(label, a, b, arrow(`s-${i}-ab`, a, b));
    ids.push(label.id, a.id, b.id);
    out.push(frame(`frame-${i}`, lane, [label.id, a.id, b.id]));
  });
  return out;
}

/** A centre and its branches. */
function mindMap() {
  const centre = box("centre", col(1) + 40, row(1), W.theIdea, { type: "ellipse", fill: PALETTE.violet, width: 200, height: 100 });
  const branches = [
    box("b1", col(0) - 20, row(0), W.why, { fill: PALETTE.blue }),
    box("b2", col(3) - 60, row(0), W.what, { fill: PALETTE.yellow }),
    box("b3", col(0) - 20, row(2), W.who, { fill: PALETTE.green }),
    box("b4", col(3) - 60, row(2), W.when, { fill: PALETTE.orange }),
  ];
  return [title("title", col(0), 20, W.mindMap), centre, ...branches, ...branches.map((b, i) => arrow(`m${i}`, centre, b))];
}

/** Three columns of cards. */
function kanban() {
  const columns = [W.toDo, W.doing, W.done];
  const out = [title("title", col(0), 20, LABEL.kanban)];
  columns.forEach((name, i) => {
    const head = box(`col-${i}`, col(i), row(0), name, { fill: PALETTE.grey, height: 50, fontSize: 18 });
    const cards = [0, 1].map((j) => note(`card-${i}-${j}`, col(i) - 10, row(0) + 80 + j * 140, t("draw-tpl-card", { n: j + 1 }), [PALETTE.yellow, PALETTE.blue, PALETTE.green][i]));
    out.push(head, ...cards, frame(`lane-${i}`, name, [head.id, ...cards.map((c) => c.id)]));
  });
  return out;
}

/** The steps a person takes, with what they feel under each. */
function userJourney() {
  const steps = [W.discover, W.signUp, W.firstUse, W.return, W.recommend];
  const out = [title("title", col(0), 20, W.userJourney)];
  const boxes = steps.map((s, i) => box(`step-${i}`, col(i), row(0), s, { fill: PALETTE.blue }));
  out.push(...boxes);
  boxes.forEach((b, i) => {
    if (i > 0) out.push(arrow(`j${i}`, boxes[i - 1], b));
    out.push(note(`feel-${i}`, col(i) - 10, row(1), W.whatTheyFeel, PALETTE.yellow));
  });
  return out;
}

/** Entities and how they relate. */
function entityRelationship() {
  const customer = box("customer", col(0), row(0), W.entityCustomer, { height: 110, fontSize: 16 });
  const order = box("order", col(2), row(0), W.entityOrder, { height: 110, fontSize: 16, fill: PALETTE.yellow });
  const line = box("line", col(2), row(2), W.entityOrderLine, { height: 110, fontSize: 16, fill: PALETTE.green });
  const product = box("product", col(0), row(2), W.entityProduct, { height: 110, fontSize: 16, fill: PALETTE.grey });
  return [
    title("title", col(0), 20, LABEL.entityRelationship),
    customer,
    order,
    line,
    product,
    arrow("r1", customer, order, "1 · n"),
    arrow("r2", order, line, "1 · n"),
    arrow("r3", product, line, "1 · n"),
  ];
}

/** A retrospective's three columns of sticky notes. */
function retrospective() {
  const columns = [
    [W.wentWell, PALETTE.green],
    [W.couldBeBetter, PALETTE.yellow],
    [W.tryNext, PALETTE.blue],
  ];
  const out = [title("title", col(0), 20, LABEL.retrospective)];
  columns.forEach(([name, fill], i) => {
    const head = box(`col-${i}`, col(i), row(0), name, { fill: PALETTE.grey, height: 50, fontSize: 18 });
    const notes = [0, 1, 2].map((j) => note(`n-${i}-${j}`, col(i) - 10, row(0) + 80 + j * 140, "…", fill));
    out.push(head, ...notes, frame(`lane-${i}`, name, [head.id, ...notes.map((n) => n.id)]));
  });
  return out;
}

/** A screen's frame with a header, a body and a footer. */
function wireframe() {
  const screen = box("screen", col(0), row(0), "", { width: 360, height: 640, fill: "#ffffff" });
  const header = box("header", col(0) + 20, row(0) + 20, W.screenTitle, { width: 320, height: 56, fill: PALETTE.grey, fontSize: 18 });
  const hero = box("hero", col(0) + 20, row(0) + 96, W.image, { width: 320, height: 200, fill: PALETTE.blue, dashed: true });
  const text1 = box("text1", col(0) + 20, row(0) + 316, W.headline, { width: 320, height: 44, fill: "#ffffff" });
  const text2 = box("text2", col(0) + 20, row(0) + 372, W.bodyCopy, { width: 320, height: 120, fill: "#ffffff", dashed: true });
  const button = box("button", col(0) + 80, row(0) + 520, W.primaryAction, { width: 200, height: 48, fill: PALETTE.violet });
  const footer = box("footer", col(0) + 20, row(0) + 588, "◦   ◦   ◦", { width: 320, height: 40, fill: PALETTE.grey, fontSize: 16 });
  return [title("title", col(0), 20, LABEL.wireframe), screen, header, hero, text1, text2, button, footer, note("hint", col(2), row(0), W.whatThisScreenIsFor, PALETTE.yellow)];
}

/** A line with milestones on it. */
function timeline() {
  const line = { id: "line", type: "line", x: col(0), y: row(1) + 40, width: col(4) - col(0), height: 0, strokeColor: "#1e1e1e", strokeWidth: 2 };
  const out = [title("title", col(0), 20, LABEL.timeline), line];
  [W.kickOff, W.alpha, W.beta, W.launch, W.review].forEach((name, i) => {
    const dot = box(`m-${i}`, col(i) - 12, row(1) + 28, "", { type: "ellipse", width: 24, height: 24, fill: PALETTE.violet });
    const label = box(`l-${i}`, col(i) - 70, row(i % 2 === 0 ? 0 : 2), name, { width: 140, height: 56, fill: i % 2 === 0 ? PALETTE.blue : PALETTE.yellow, fontSize: 16 });
    out.push(dot, label, arrow(`c-${i}`, label, dot));
  });
  return out;
}

/** The templates, *Empty* first, in the order the gallery shows them. */
export const TEMPLATES = Object.freeze([
  { id: "empty", label: LABEL.empty, blurb: t("draw-template-empty-blurb"), skeleton: empty },
  { id: "system-architecture", label: LABEL.systemArchitecture, blurb: t("draw-template-system-architecture-blurb"), skeleton: systemArchitecture },
  { id: "flowchart", label: LABEL.flowchart, blurb: t("draw-template-flowchart-blurb"), skeleton: flowchart },
  { id: "swimlanes", label: LABEL.swimlanes, blurb: t("draw-template-swimlanes-blurb"), skeleton: swimlanes },
  { id: "mind-map", label: LABEL.mindMap, blurb: t("draw-template-mind-map-blurb"), skeleton: mindMap },
  { id: "kanban", label: LABEL.kanban, blurb: t("draw-template-kanban-blurb"), skeleton: kanban },
  { id: "user-journey", label: LABEL.userJourney, blurb: t("draw-template-user-journey-blurb"), skeleton: userJourney },
  { id: "entity-relationship", label: LABEL.entityRelationship, blurb: t("draw-template-entity-relationship-blurb"), skeleton: entityRelationship },
  { id: "retrospective", label: LABEL.retrospective, blurb: t("draw-template-retrospective-blurb"), skeleton: retrospective },
  { id: "wireframe", label: LABEL.wireframe, blurb: t("draw-template-wireframe-blurb"), skeleton: wireframe },
  { id: "timeline", label: LABEL.timeline, blurb: t("draw-template-timeline-blurb"), skeleton: timeline },
]);

/** The template the gallery picked, by id; *Empty* for an unknown one. */
export function templateOf(id) {
  return TEMPLATES.find((tpl) => tpl.id === id) ?? TEMPLATES[0];
}
