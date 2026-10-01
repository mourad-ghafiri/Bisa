/**
 * A deck's outline. Run with `node --test desktop/src/ui/artifact/pptxModel.test.mjs`.
 */
import { strict as assert } from "node:assert";
import { test } from "node:test";

import { notesPath, notesText, resolveEntry, slideImages, slidePaths, slideRelsPath, slideText, slideWords } from "./pptxModel.mjs";

test("slides are found in order, whatever the zip's order", () => {
  const names = ["ppt/slides/slide10.xml", "ppt/slides/_rels/slide1.xml.rels", "ppt/slides/slide2.xml", "ppt/slides/slide1.xml", "ppt/media/image1.png", "ppt/slideLayouts/slideLayout1.xml"];
  assert.deepEqual(slidePaths(names), ["ppt/slides/slide1.xml", "ppt/slides/slide2.xml", "ppt/slides/slide10.xml"]);
  assert.equal(slideRelsPath("ppt/slides/slide3.xml"), "ppt/slides/_rels/slide3.xml.rels");
  assert.equal(slideWords(1), "1 slide");
  assert.equal(slideWords(12), "12 slides");
});

test("a slide's title comes from its title shape, else its first paragraph; runs join, entities decode", () => {
  const xml = `<p:sld><p:cSld><p:spTree>
    <p:sp><p:nvSpPr><p:nvPr><p:ph type="ctrTitle"/></p:nvPr></p:nvSpPr><p:txBody><a:p><a:r><a:t>Launch </a:t></a:r><a:r><a:t>plan &amp; dates</a:t></a:r></a:p></p:txBody></p:sp>
    <p:sp><p:txBody><a:p><a:r><a:t>First point</a:t></a:r></a:p><a:p><a:r><a:t>Second</a:t></a:r><a:br/><a:r><a:t> point</a:t></a:r></a:p><a:p></a:p></p:txBody></p:sp>
  </p:spTree></p:cSld></p:sld>`;
  assert.deepEqual(slideText(xml), { title: "Launch plan & dates", paragraphs: ["First point", "Second point"] });
  const untitled = `<p:sp><p:txBody><a:p><a:r><a:t>Only words</a:t></a:r></a:p><a:p><a:r><a:t>More</a:t></a:r></a:p></p:txBody></p:sp>`;
  assert.deepEqual(slideText(untitled), { title: "Only words", paragraphs: ["More"] });
  assert.deepEqual(slideText(""), { title: "", paragraphs: [] });
});

test("pictures and notes are the slide's rels, resolved against its folder", () => {
  const rels = `<Relationships>
    <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/>
    <Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../media/image1.png"/>
    <Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/notesSlide" Target="../notesSlides/notesSlide1.xml"/>
    <Relationship Id="rId4" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="/ppt/media/image2.jpeg"/>
  </Relationships>`;
  assert.deepEqual(slideImages(rels, "ppt/slides/slide1.xml"), ["ppt/media/image1.png", "ppt/media/image2.jpeg"]);
  assert.equal(notesPath(rels, "ppt/slides/slide1.xml"), "ppt/notesSlides/notesSlide1.xml");
  assert.equal(notesPath("<Relationships/>", "ppt/slides/slide1.xml"), null);
  assert.equal(resolveEntry("ppt/slides/slide1.xml", "./x.xml"), "ppt/slides/x.xml");
  assert.equal(notesText("<p:notes><a:p><a:r><a:t>Say hello</a:t></a:r></a:p><a:p><a:r><a:t>then pause</a:t></a:r></a:p></p:notes>"), "Say hello\nthen pause");
  assert.equal(notesText(null), "");
});
