/**
 * The text kinds (ide/12): Markdown rendered, a Mermaid source drawn, code
 * and data in the editor read-only with the language its name says — plain
 * above a megabyte — and a page's or a figure's source when asked for.
 */

import { CodeEditor } from "../CodeEditor";
import { cn } from "../cn";
import { Markdown } from "../Markdown";
import { MermaidView } from "../MermaidView";
import { languagePath, PLAIN_TEXT_BYTES } from "./artifactModel.mjs";
import type { ArtifactRef } from "../../types";

export function TextView({ artifact, text, className }: { artifact: ArtifactRef; text: string; className?: string }) {
  if (artifact.kind === "markdown") {
    return (
      <div data-scroll-keep="text" className={cn("h-full overflow-auto p-4", className)}>
        <Markdown text={text} />
      </div>
    );
  }
  if (artifact.kind === "diagram") {
    return <MermaidView source={text} exportName={artifact.title} className={cn("h-full", className)} />;
  }
  return (
    <CodeEditor
      value={text}
      path={languagePath(artifact)}
      readOnly
      plain={artifact.size > PLAIN_TEXT_BYTES}
      className={cn("h-full", className)}
    />
  );
}
