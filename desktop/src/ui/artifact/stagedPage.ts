/**
 * A page an agent wrote, loaded from the shell's own scheme (12 — Artifacts
 * § Security; `src-tauri/src/pages.rs`). A `srcdoc` document inherits its
 * parent's Content-Security-Policy (CSP3 § 7.8), and the app's page carries
 * one that allows no inline script — so in the desktop app the document is
 * **staged** with the shell (`page_stage`) and the frame loads it as
 * `bisa-page://page/<id>/`: a document under its own scheme, with its own
 * policy as a header, an opaque origin and no IPC. The page is released when
 * the frame goes or the document changes (`page_release`). In a browser — a
 * dev session — there is no shell and no policy to inherit, so the frame
 * keeps `srcdoc`, which is also where a page lands if the shell refuses to
 * stage it: shown, said in the log, under whatever policy the window has.
 */

import { useEffect, useState } from "react";
import { inDesktopShell } from "../../api";
import { errorFields, log } from "../../log";
import { stagedPageId } from "./pageDocument.mjs";

/** What the frame loads: the staged URL, the document itself, or nothing yet. */
export type StagedPage = { src: string } | { srcDoc: string } | null;

export function useStagedPage(doc: string, csp: string): StagedPage {
  const [page, setPage] = useState<StagedPage>(() => (inDesktopShell() ? null : { srcDoc: doc }));
  useEffect(() => {
    if (!inDesktopShell()) {
      setPage({ srcDoc: doc });
      return;
    }
    let live = true;
    let staged: string | null = null;
    setPage(null);
    const release = (url: string) =>
      import("@tauri-apps/api/core")
        .then(({ invoke }) => invoke("page_release", { id: stagedPageId(url) }))
        .catch((e: unknown) => log.debug("artifact", "a staged page could not be released", errorFields(e)));
    void import("@tauri-apps/api/core")
      .then(({ invoke }) => invoke<string>("page_stage", { html: doc, csp }))
      .then((url) => {
        if (!live) return void release(url);
        staged = url;
        setPage({ src: url });
      })
      .catch((e: unknown) => {
        log.warn("artifact", "the shell did not stage the page; shown as srcdoc", errorFields(e));
        if (live) setPage({ srcDoc: doc });
      });
    return () => {
      live = false;
      if (staged) void release(staged);
    };
  }, [doc, csp]);
  return page;
}
