//! `Drawing`: a picture on a canvas (19 — Drawings). The record — its title,
//! what it is attached to ([`OwnerScope`]) and its **scene** — is a GEP
//! record of kind [`crate::kind::KIND_DRAWING`], so it travels; the scene is
//! Excalidraw's element JSON, which the desktop's canvas owns and this crate
//! only bounds: every element is one of a few vector types, under a size and
//! a count a record can carry whole to a peer, and never an image.
//!
//! What every agent is told about the canvas is here too, in one place —
//! the tools and the note — because the engine and the MCP server both
//! depend on this crate and on nothing of each other, as with
//! [`crate::browser`].

use crate::id::DrawingId;
use crate::owner_scope::OwnerScope;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// Longest scene, serialized — a record must cross the wire whole, and one
/// frame of the sync carries a mebibyte with the envelope around it.
pub const MAX_SCENE_BYTES: usize = 768 * 1024;
/// Most elements in one drawing.
pub const MAX_DRAWING_ELEMENTS: usize = 4000;
/// Most skeleton elements one `drawing_draw` call adds.
pub const MAX_SKELETON_PER_CALL: usize = 500;
/// Longest text on one element — a label, a text box, a sticky note.
pub const MAX_ELEMENT_TEXT_BYTES: usize = 4 * 1024;
/// Longest reading an agent gets of a scene ([`Scene::describe`]).
pub const MAX_DESCRIBE_BYTES: usize = 16 * 1024;
/// Longest title, in characters — a line in a list.
pub const MAX_DRAWING_TITLE_CHARS: usize = 120;

/// The element types a scene may hold — the vector ones. The canvas draws
/// nothing else, and an agent's skeleton is checked against the same list.
pub const ELEMENT_TYPES: &[&str] = &[
    "rectangle",
    "ellipse",
    "diamond",
    "arrow",
    "line",
    "freedraw",
    "text",
    "frame",
];

/// The element types refused by name: each carries bytes or a page from
/// outside, and a record that travels carries neither.
pub const REFUSED_ELEMENT_TYPES: &[&str] = &["image", "embeddable", "iframe", "magicframe"];

/// Every drawing tool, in the order the note names them — the one list the
/// note, the skill and the reference are checked against.
pub const DRAW_TOOLS: &[&str] = &[
    "drawing_list",
    "drawing_read",
    "drawing_create",
    "drawing_draw",
    "drawing_mermaid",
    "drawing_erase",
    "drawing_snapshot",
];

/// The sentence every session reads about the canvas: the tools, the order
/// to use them in, what a refusal means, and the one licence to stop. How
/// to compose a drawing is the Drawing skill's.
pub const DRAW_NOTE: &str = "Drawings: use the drawing tools, which draw on the platform's canvas where \
the person watches each change land — drawing_list the drawings of a scope, drawing_create one with a \
title, drawing_read a drawing before you touch it (every element with its id, type, text and place), \
drawing_draw to add elements written as a skeleton (rectangle, ellipse, diamond, text, arrow, line, \
frame; an arrow binds by the ids you give; replace: true redraws the whole drawing), drawing_mermaid to \
draw a flowchart written as Mermaid, drawing_erase to remove elements by id, drawing_snapshot to look \
at the result — the PNG's path is answered, read that file to see it. In a conversation about a \
drawing the drawing is chosen for you. Never an image: the canvas is vector only. When a drawing tool \
refuses you, answers that the canvas is not available, or names a platform fault, say so to the \
person once and stop — never work around it, and never ask twice whether to retry.";

/// The part of the canvas's state a drawing keeps: the ground it is drawn on
/// and whether the grid shows. Everything else the canvas holds — the
/// selection, the zoom, the open menu — is one window's and never saved.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SceneAppState {
    #[serde(default = "default_background")]
    pub view_background_color: String,
    #[serde(default)]
    pub grid: bool,
}

fn default_background() -> String {
    "#ffffff".to_string()
}

impl Default for SceneAppState {
    fn default() -> Self {
        Self {
            view_background_color: default_background(),
            grid: false,
        }
    }
}

/// The picture: Excalidraw's elements as the canvas wrote them, and the
/// little of its state a drawing keeps. The elements are JSON on purpose —
/// the canvas owns their shape and this crate checks only what it promises
/// ([`Scene::validate`]).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Scene {
    #[serde(default)]
    pub elements: Vec<Value>,
    #[serde(default)]
    pub app_state: SceneAppState,
}

impl Scene {
    pub fn empty() -> Self {
        Self::default()
    }

    /// Every element is an object of a vector type with an id and a place,
    /// its text within the cap; the scene within its count and its size.
    pub fn validate(&self) -> Result<(), DrawError> {
        if self.elements.len() > MAX_DRAWING_ELEMENTS {
            return Err(DrawError::TooManyElements {
                count: self.elements.len(),
            });
        }
        for element in &self.elements {
            check_element(element)?;
        }
        let bytes = serde_json::to_vec(self)
            .map(|v| v.len())
            .unwrap_or(usize::MAX);
        if bytes > MAX_SCENE_BYTES {
            return Err(DrawError::TooLarge { bytes });
        }
        Ok(())
    }

    /// The elements that are drawn — the canvas keeps a deleted element for
    /// its undo, a record does not.
    pub fn without_deleted(mut self) -> Self {
        self.elements.retain(|e| !is_deleted(e));
        self
    }

    pub fn element_count(&self) -> usize {
        self.elements.iter().filter(|e| !is_deleted(e)).count()
    }

    /// Remove the elements named, and what only made sense with them: a
    /// label bound inside one goes too, an arrow that ended on one loses
    /// that end's binding, a child of a removed frame stands free. Answers
    /// how many elements left.
    pub fn erase(&mut self, ids: &[String]) -> usize {
        let named: HashSet<&str> = ids.iter().map(String::as_str).collect();
        let gone: HashSet<String> = self
            .elements
            .iter()
            .filter(|e| {
                id_of(e).is_some_and(|id| named.contains(id))
                    || str_field(e, "containerId").is_some_and(|c| named.contains(c))
            })
            .filter_map(|e| id_of(e).map(str::to_string))
            .collect();
        let before = self.elements.len();
        self.elements
            .retain(|e| !id_of(e).is_some_and(|id| gone.contains(id)));
        for element in &mut self.elements {
            let Some(obj) = element.as_object_mut() else {
                continue;
            };
            if let Some(Value::Array(bound)) = obj.get_mut("boundElements") {
                bound.retain(|b| !str_field(b, "id").is_some_and(|id| gone.contains(id)));
            }
            for key in ["startBinding", "endBinding"] {
                let dangling = obj
                    .get(key)
                    .and_then(|b| str_field(b, "elementId"))
                    .is_some_and(|id| gone.contains(id));
                if dangling {
                    obj.insert(key.to_string(), Value::Null);
                }
            }
            if str_field(element, "frameId").is_some_and(|f| gone.contains(f)) {
                if let Some(obj) = element.as_object_mut() {
                    obj.insert("frameId".to_string(), Value::Null);
                }
            }
        }
        before - self.elements.len()
    }

    /// The scene as an agent reads it: one line per drawn element — its id,
    /// its type, its text, where it is and how big — an arrow with the ids
    /// it runs between, a frame with its name; cut at [`MAX_DESCRIBE_BYTES`]
    /// with a line saying how many more there are.
    pub fn describe(&self) -> String {
        // A label lives in a text element bound to its container: read it
        // onto the container's line, and skip the text element itself.
        let mut labels: HashMap<&str, &str> = HashMap::new();
        for e in &self.elements {
            if str_field(e, "type") == Some("text") {
                if let (Some(container), Some(text)) =
                    (str_field(e, "containerId"), str_field(e, "text"))
                {
                    labels.insert(container, text);
                }
            }
        }
        let drawn: Vec<&Value> = self
            .elements
            .iter()
            .filter(|e| !is_deleted(e))
            .filter(|e| {
                !(str_field(e, "type") == Some("text") && str_field(e, "containerId").is_some())
            })
            .collect();
        let mut out = String::new();
        let mut shown = 0usize;
        for e in &drawn {
            let line = describe_element(e, &labels);
            if out.len() + line.len() + 1 > MAX_DESCRIBE_BYTES {
                break;
            }
            out.push_str(&line);
            out.push('\n');
            shown += 1;
        }
        if shown < drawn.len() {
            out.push_str(&format!(
                "… {} more elements not shown\n",
                drawn.len() - shown
            ));
        }
        if drawn.is_empty() {
            out.push_str("(the canvas is empty)\n");
        }
        out
    }
}

fn describe_element(e: &Value, labels: &HashMap<&str, &str>) -> String {
    let id = id_of(e).unwrap_or("?");
    let kind = str_field(e, "type").unwrap_or("?");
    let mut line = format!("{id} {kind}");
    let text = str_field(e, "text")
        .or_else(|| labels.get(id).copied())
        .or_else(|| str_field(e, "name"));
    if let Some(text) = text.filter(|t| !t.trim().is_empty()) {
        let one_line = text.split(['\n', '\r']).collect::<Vec<_>>().join(" ");
        line.push_str(&format!(" {one_line:?}"));
    }
    let (x, y) = (num_field(e, "x"), num_field(e, "y"));
    let (w, h) = (num_field(e, "width"), num_field(e, "height"));
    line.push_str(&format!(" at {x},{y} {w}×{h}"));
    if matches!(kind, "arrow" | "line") {
        let from = e
            .get("startBinding")
            .and_then(|b| str_field(b, "elementId"));
        let to = e.get("endBinding").and_then(|b| str_field(b, "elementId"));
        if from.is_some() || to.is_some() {
            line.push_str(&format!(
                " from {} to {}",
                from.unwrap_or("nothing"),
                to.unwrap_or("nothing")
            ));
        }
    }
    if let Some(frame) = str_field(e, "frameId") {
        line.push_str(&format!(" in frame {frame}"));
    }
    line
}

fn id_of(e: &Value) -> Option<&str> {
    str_field(e, "id")
}

fn str_field<'a>(e: &'a Value, key: &str) -> Option<&'a str> {
    e.get(key).and_then(Value::as_str)
}

fn num_field(e: &Value, key: &str) -> i64 {
    e.get(key)
        .and_then(Value::as_f64)
        .map(|n| n.round() as i64)
        .unwrap_or(0)
}

fn is_deleted(e: &Value) -> bool {
    e.get("isDeleted").and_then(Value::as_bool).unwrap_or(false)
}

/// One element of a scene as the canvas wrote it: an object of a vector
/// type with a non-empty id and numeric coordinates, its text bounded.
fn check_element(e: &Value) -> Result<(), DrawError> {
    let Some(obj) = e.as_object() else {
        return Err(DrawError::BadElement {
            why: "not an object".into(),
        });
    };
    check_type(e)?;
    match obj.get("id").and_then(Value::as_str) {
        Some(id) if !id.is_empty() && id.len() <= 64 => {}
        _ => {
            return Err(DrawError::BadElement {
                why: "an element needs an id of at most 64 characters".into(),
            })
        }
    }
    for key in ["x", "y"] {
        if !obj.get(key).is_some_and(Value::is_number) {
            return Err(DrawError::BadElement {
                why: format!("`{key}` is not a number"),
            });
        }
    }
    check_texts(e)
}

/// The type is one of the vector ones; the refused ones are named in the
/// refusal, anything else is not an element the canvas knows.
fn check_type(e: &Value) -> Result<(), DrawError> {
    match str_field(e, "type") {
        Some(t) if ELEMENT_TYPES.contains(&t) => Ok(()),
        Some(t) if REFUSED_ELEMENT_TYPES.contains(&t) => Err(DrawError::ElementRefused {
            kind: t.to_string(),
        }),
        Some(t) => Err(DrawError::BadElement {
            why: format!("{t:?} is not an element type the canvas draws"),
        }),
        None => Err(DrawError::BadElement {
            why: "an element needs a type".into(),
        }),
    }
}

/// Every text an element carries — its own, its original, a skeleton's
/// label — is within the cap.
fn check_texts(e: &Value) -> Result<(), DrawError> {
    let texts = [
        str_field(e, "text"),
        str_field(e, "originalText"),
        e.get("label").and_then(|l| str_field(l, "text")),
        e.get("label").and_then(Value::as_str),
    ];
    for text in texts.into_iter().flatten() {
        if text.len() > MAX_ELEMENT_TEXT_BYTES {
            return Err(DrawError::BadElement {
                why: format!(
                    "a text of {} bytes; the cap is {MAX_ELEMENT_TEXT_BYTES}",
                    text.len()
                ),
            });
        }
    }
    Ok(())
}

/// What an agent hands `drawing_draw`: a list of skeleton elements — a type,
/// a place, and what the canvas needs to make the rest (a label, a binding,
/// a frame's children). Bounded in count and text like a scene's, of the
/// same types; the canvas fills in the ids it does not find.
pub fn validate_skeleton(elements: &[Value]) -> Result<(), DrawError> {
    if elements.len() > MAX_SKELETON_PER_CALL {
        return Err(DrawError::TooManyElements {
            count: elements.len(),
        });
    }
    for e in elements {
        if !e.is_object() {
            return Err(DrawError::BadElement {
                why: "not an object".into(),
            });
        }
        check_type(e)?;
        check_texts(e)?;
    }
    Ok(())
}

/// The record: what travels as kind 33401.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Drawing {
    pub id: DrawingId,
    pub scope: OwnerScope,
    pub title: String,
    #[serde(default)]
    pub pinned: bool,
    pub created_at: u64,
    pub updated_at: u64,
    pub scene: Scene,
}

impl Drawing {
    pub fn validate(&self) -> Result<(), DrawError> {
        let title = self.title.trim();
        if title.is_empty() {
            return Err(DrawError::EmptyTitle);
        }
        let chars = title.chars().count();
        if chars > MAX_DRAWING_TITLE_CHARS {
            return Err(DrawError::TitleTooLong { chars });
        }
        self.scene.validate()
    }
}

/// A drawing as a list draws it: every column but the scene, which a list
/// never carries.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DrawingSummary {
    pub id: DrawingId,
    pub scope: OwnerScope,
    pub title: String,
    pub pinned: bool,
    /// The scene's hash — what an edit passes back as its `base_hash`.
    pub hash: String,
    pub element_count: usize,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DrawError {
    #[error("a drawing needs a title")]
    EmptyTitle,
    #[error("a drawing's title is {chars} characters; the cap is {MAX_DRAWING_TITLE_CHARS}")]
    TitleTooLong { chars: usize },
    #[error("the scene is {bytes} bytes; the cap is {MAX_SCENE_BYTES}")]
    TooLarge { bytes: usize },
    #[error("{count} elements; the cap is {MAX_DRAWING_ELEMENTS} in a drawing and {MAX_SKELETON_PER_CALL} in one call")]
    TooManyElements { count: usize },
    /// A type the canvas refuses by name — an image, an embedded page.
    #[error("the canvas draws no {kind}: it is vector only")]
    ElementRefused { kind: String },
    #[error("not an element the canvas draws: {why}")]
    BadElement { why: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn shape(id: &str, kind: &str) -> Value {
        json!({"id": id, "type": kind, "x": 10, "y": 20, "width": 160, "height": 80})
    }

    fn arrow(id: &str, from: &str, to: &str) -> Value {
        json!({"id": id, "type": "arrow", "x": 0, "y": 0, "width": 100, "height": 0,
               "startBinding": {"elementId": from}, "endBinding": {"elementId": to}})
    }

    fn drawing(scene: Scene) -> Drawing {
        Drawing {
            id: DrawingId::from_ulid(ulid::Ulid::from_parts(7, 1)),
            scope: OwnerScope::Workspace,
            title: "Where the data goes".into(),
            pinned: false,
            created_at: 1,
            updated_at: 2,
            scene,
        }
    }

    #[test]
    fn every_drawing_tool_is_prefixed_and_named_in_the_note() {
        assert_eq!(DRAW_TOOLS.len(), 7);
        for tool in DRAW_TOOLS {
            assert!(tool.starts_with("drawing_"), "{tool}");
            assert!(DRAW_NOTE.contains(tool), "the note names {tool}");
        }
        let mut sorted = DRAW_TOOLS.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), DRAW_TOOLS.len(), "no tool twice");
    }

    #[test]
    fn a_scene_of_vector_elements_within_its_bounds_is_valid_and_an_image_is_refused_by_name() {
        let mut scene = Scene::empty();
        scene.elements = vec![
            shape("a", "rectangle"),
            shape("b", "ellipse"),
            arrow("ab", "a", "b"),
        ];
        assert!(drawing(scene.clone()).validate().is_ok());
        scene.elements.push(shape("img", "image"));
        assert_eq!(
            drawing(scene).validate(),
            Err(DrawError::ElementRefused {
                kind: "image".into()
            })
        );
        let mut odd = Scene::empty();
        odd.elements = vec![json!({"id": "x", "type": "hexagon", "x": 0, "y": 0})];
        assert!(matches!(
            odd.validate(),
            Err(DrawError::BadElement { why }) if why.contains("hexagon")
        ));
        let mut placeless = Scene::empty();
        placeless.elements = vec![json!({"id": "x", "type": "rectangle", "x": "left", "y": 0})];
        assert!(matches!(
            placeless.validate(),
            Err(DrawError::BadElement { .. })
        ));
        let mut nameless = Scene::empty();
        nameless.elements = vec![json!({"type": "rectangle", "x": 0, "y": 0})];
        assert!(matches!(
            nameless.validate(),
            Err(DrawError::BadElement { .. })
        ));
    }

    #[test]
    fn a_scene_over_its_size_or_its_count_is_refused_and_so_is_a_bad_title() {
        let mut many = Scene::empty();
        many.elements = (0..=MAX_DRAWING_ELEMENTS)
            .map(|i| shape(&format!("e{i}"), "rectangle"))
            .collect();
        assert_eq!(
            many.validate(),
            Err(DrawError::TooManyElements {
                count: MAX_DRAWING_ELEMENTS + 1
            })
        );
        let mut heavy = Scene::empty();
        let mut e = shape("t", "text");
        e["text"] = json!("x".repeat(MAX_ELEMENT_TEXT_BYTES + 1));
        heavy.elements = vec![e];
        assert!(matches!(
            heavy.validate(),
            Err(DrawError::BadElement { .. })
        ));
        let mut big = Scene::empty();
        big.elements = (0..400)
            .map(|i| {
                let mut e = shape(&format!("e{i}"), "text");
                e["text"] = json!("y".repeat(MAX_ELEMENT_TEXT_BYTES));
                e
            })
            .collect();
        assert!(matches!(big.validate(), Err(DrawError::TooLarge { .. })));
        let mut untitled = drawing(Scene::empty());
        untitled.title = "  ".into();
        assert_eq!(untitled.validate(), Err(DrawError::EmptyTitle));
        untitled.title = "t".repeat(MAX_DRAWING_TITLE_CHARS + 1);
        assert!(matches!(
            untitled.validate(),
            Err(DrawError::TitleTooLong { .. })
        ));
    }

    #[test]
    fn erasing_takes_the_label_with_the_box_and_repairs_what_pointed_at_it() {
        let mut scene = Scene::empty();
        let mut a = shape("a", "rectangle");
        a["boundElements"] =
            json!([{"id": "a-label", "type": "text"}, {"id": "ab", "type": "arrow"}]);
        let mut label = shape("a-label", "text");
        label["containerId"] = json!("a");
        label["text"] = json!("API");
        let mut b = shape("b", "ellipse");
        b["boundElements"] = json!([{"id": "ab", "type": "arrow"}]);
        let mut child = shape("c", "diamond");
        child["frameId"] = json!("f");
        let frame = shape("f", "frame");
        scene.elements = vec![a, label, b, arrow("ab", "a", "b"), child, frame];
        let removed = scene.erase(&["a".to_string(), "f".to_string()]);
        assert_eq!(removed, 3, "the box, its label and the frame");
        let ids: Vec<&str> = scene.elements.iter().filter_map(id_of).collect();
        assert_eq!(ids, ["b", "ab", "c"]);
        let arrow = &scene.elements[1];
        assert!(
            arrow["startBinding"].is_null(),
            "the end that pointed at a is loose"
        );
        assert_eq!(arrow["endBinding"]["elementId"], "b");
        assert_eq!(
            scene.elements[0]["boundElements"].as_array().unwrap().len(),
            1
        );
        assert!(
            scene.elements[2]["frameId"].is_null(),
            "the child stands free"
        );
    }

    #[test]
    fn a_reading_names_every_drawn_element_with_its_label_and_its_ends_and_is_bounded() {
        let mut scene = Scene::empty();
        let mut a = shape("a", "rectangle");
        a["boundElements"] = json!([{"id": "a-label", "type": "text"}]);
        let mut label = shape("a-label", "text");
        label["containerId"] = json!("a");
        label["text"] = json!("API\ngateway");
        let mut gone = shape("z", "ellipse");
        gone["isDeleted"] = json!(true);
        scene.elements = vec![a, label, shape("b", "ellipse"), arrow("ab", "a", "b"), gone];
        let text = scene.describe();
        assert!(
            text.contains("a rectangle \"API gateway\" at 10,20 160×80"),
            "{text}"
        );
        assert!(text.contains("ab arrow at 0,0 100×0 from a to b"), "{text}");
        assert!(
            !text.contains("a-label"),
            "a label reads on its box: {text}"
        );
        assert!(
            !text.contains("\nz "),
            "a deleted element is not drawn: {text}"
        );
        assert_eq!(scene.element_count(), 4);
        assert_eq!(Scene::empty().describe(), "(the canvas is empty)\n");

        let mut crowd = Scene::empty();
        crowd.elements = (0..2000)
            .map(|i| shape(&format!("element-{i}"), "rectangle"))
            .collect();
        let text = crowd.describe();
        assert!(text.len() <= MAX_DESCRIBE_BYTES + 64, "{}", text.len());
        assert!(text.contains("more elements not shown"));
    }

    #[test]
    fn a_skeleton_is_checked_for_its_types_its_texts_and_its_count() {
        assert!(validate_skeleton(&[
            json!({"type": "rectangle", "x": 0, "y": 0, "label": {"text": "Service"}}),
            json!({"type": "arrow", "x": 0, "y": 0, "start": {"id": "s"}, "end": {"id": "d"}}),
        ])
        .is_ok());
        assert_eq!(
            validate_skeleton(&[json!({"type": "image", "x": 0, "y": 0})]),
            Err(DrawError::ElementRefused {
                kind: "image".into()
            })
        );
        assert!(matches!(
            validate_skeleton(&[
                json!({"type": "text", "x": 0, "y": 0, "text": "x".repeat(MAX_ELEMENT_TEXT_BYTES + 1)})
            ]),
            Err(DrawError::BadElement { .. })
        ));
        let many: Vec<Value> = (0..=MAX_SKELETON_PER_CALL)
            .map(|_| json!({"type": "rectangle", "x": 0, "y": 0}))
            .collect();
        assert!(matches!(
            validate_skeleton(&many),
            Err(DrawError::TooManyElements { .. })
        ));
        assert!(matches!(
            validate_skeleton(&[json!("not an object")]),
            Err(DrawError::BadElement { .. })
        ));
    }

    #[test]
    fn the_record_round_trips_and_a_scene_defaults_to_an_empty_white_canvas() {
        let d = drawing(Scene::empty());
        let json = serde_json::to_value(&d).unwrap();
        assert_eq!(json["scope"]["kind"], "workspace");
        assert_eq!(
            json["scene"]["app_state"]["view_background_color"],
            "#ffffff"
        );
        assert_eq!(serde_json::from_value::<Drawing>(json).unwrap(), d);
        let bare: Scene = serde_json::from_value(json!({})).unwrap();
        assert_eq!(bare, Scene::empty());
        assert!(!bare.app_state.grid);
    }
}
