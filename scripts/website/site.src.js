/**
 * bisa.dev's few behaviours (website/README.md). Every page reads whole
 * without them; they add what a script can: the tour's rail, shown once the
 * reader is past the opening and following them, the opening's slideshow, the reading line, arrows
 * for the IDE's panels, the asks' filter, Moonrice waking near its step, and links that
 * open a page's own file when the site is read straight from the disk. The
 * rules live in railModel.mjs, with no DOM in them; `build.mjs` bundles the
 * two into `website/assets/site.js`, one plain script that also runs from a
 * `file:` address.
 */
import { chipStates, counterWords, currentStep, nextSlide, previousSlide, progress, slideAt } from "./railModel.mjs";

const reduced = window.matchMedia("(prefers-reduced-motion: reduce)");

/** The rail and the reading line, measured once a frame at most. */
function followTheRun() {
  const line = document.querySelector(".reading span");
  const sections = [...document.querySelectorAll("[data-step]")];
  const rail = document.querySelector(".rail");
  const chips = [...document.querySelectorAll(".rail [data-rail]")];
  const counter = document.querySelector(".rail-counter");
  let queued = false;
  const paint = () => {
    queued = false;
    const doc = document.documentElement;
    line?.style.setProperty("--progress", String(progress(doc.scrollTop, doc.scrollHeight, window.innerHeight)));
    if (!chips.length) return;
    const tops = sections.map((s) => s.getBoundingClientRect().top);
    const at = currentStep(tops, window.innerHeight * 0.4);
    // The opening presents the platform; the rail comes in with the tour.
    rail?.classList.toggle("is-shown", at >= 0);
    const states = chipStates(chips.length, at);
    chips.forEach((chip, i) => {
      chip.dataset.state = states[i];
      if (i === at) chip.setAttribute("aria-current", "step");
      else chip.removeAttribute("aria-current");
    });
    if (counter) counter.textContent = counterWords(at, chips.length);
  };
  const ask = () => {
    if (queued) return;
    queued = true;
    requestAnimationFrame(paint);
  };
  window.addEventListener("scroll", ask, { passive: true });
  window.addEventListener("resize", ask, { passive: true });
  paint();
}

/** Arrows beside a track that scrolls sideways, one card at a time. */
function trackArrows() {
  for (const track of document.querySelectorAll(".ide-track")) {
    const bar = document.createElement("div");
    bar.className = "wrap track-arrows";
    for (const [label, sign, glyph] of [["Previous panel", -1, "←"], ["Next panel", 1, "→"]]) {
      const button = document.createElement("button");
      button.type = "button";
      button.setAttribute("aria-label", label);
      button.textContent = glyph;
      button.addEventListener("click", () => {
        const card = track.querySelector(".ide-card");
        const step = card ? card.getBoundingClientRect().width + 20 : track.clientWidth * 0.8;
        track.scrollBy({ left: sign * step, behavior: reduced.matches ? "auto" : "smooth" });
      });
      bar.append(button);
    }
    track.after(bar);
  }
}

/** The hundred asks, shown by what they need. */
function filterAsks() {
  const asks = document.querySelector(".asks");
  const bar = asks?.querySelector(".asks-filter");
  if (!bar) return;
  bar.hidden = false;
  bar.addEventListener("click", (e) => {
    const button = e.target.closest("button[data-filter]");
    if (!button) return;
    asks.dataset.filter = button.dataset.filter;
    for (const b of bar.querySelectorAll("button")) b.setAttribute("aria-pressed", String(b === button));
  });
}

/** Moonrice's sheet is fetched only when its step comes near. */
function wakeThePet() {
  const pets = document.querySelectorAll(".pet");
  if (!pets.length) return;
  const near = new IntersectionObserver(
    (entries) => {
      for (const e of entries) {
        if (!e.isIntersecting) continue;
        e.target.classList.add("is-awake");
        near.unobserve(e.target);
      }
    },
    { rootMargin: "300px" },
  );
  for (const pet of pets) near.observe(pet);
}

/**
 * Read straight from the disk, a link to a folder (`features/`) would open
 * the folder: it is pointed at the folder's page instead. A served site keeps
 * its clean addresses.
 */
function linksOnDisk() {
  if (location.protocol !== "file:") return;
  for (const a of document.querySelectorAll("a[href]")) {
    const href = a.getAttribute("href");
    if (/^(https?:|mailto:|#)/.test(href)) continue;
    const [path, hash] = href.split("#");
    if (path === "" || !path.endsWith("/")) continue;
    a.setAttribute("href", `${path}index.html${hash === undefined ? "" : `#${hash}`}`);
  }
}

/**
 * The opening's slideshow: it moves on to the next screen by itself every few seconds, round and
 * round — unless motion is reduced or the page is hidden. Two arrows, shown while the pointer is over
 * it (and always where there is no pointer to hover), go back and on; the clock starts again from
 * there. Without this it is a strip of screens to swipe.
 */
function playSlides() {
  const track = document.querySelector("[data-slides] .slides-track");
  const count = track ? track.children.length : 0;
  if (count < 2) return;
  let at = 0;
  let last = Date.now();
  const go = (i) => {
    at = i;
    last = Date.now();
    track.scrollTo({ left: i * track.clientWidth, behavior: reduced.matches ? "auto" : "smooth" });
  };
  for (const [label, side, glyph, to] of [
    ["Previous screen", "prev", "←", () => previousSlide(at, count)],
    ["Next screen", "next", "→", () => nextSlide(at, count)],
  ]) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = `slides-arrow ${side}`;
    button.setAttribute("aria-label", label);
    button.textContent = glyph;
    button.addEventListener("click", () => go(to()));
    track.parentElement.append(button);
  }
  let pending = false;
  track.addEventListener(
    "scroll",
    () => {
      if (pending) return;
      pending = true;
      requestAnimationFrame(() => {
        pending = false;
        at = slideAt(track.scrollLeft, track.clientWidth, count);
      });
    },
    { passive: true },
  );
  setInterval(() => {
    if (reduced.matches || document.hidden || Date.now() - last < 5500) return;
    go(nextSlide(at, count));
  }, 6000);
}

linksOnDisk();
playSlides();
followTheRun();
trackArrows();
filterAsks();
wakeThePet();
