//! Every workflow template of the catalog, installed and run through the
//! binary to its end — what a person gets when they pick one from the
//! library. A template is installed through the node with the agents it
//! names; its agents are put on the harness this machine has; a goal is
//! captured on it with the inputs its form asks for; scripted agents yield
//! what each step's schema asks; a person answers what is asked of them; and
//! the run comes to its end with every step of its main way done.
//!
//! Nothing here is written per template but what its person gives it and
//! which steps its main way passes by: the agents' turns, their results and
//! the answers are read off the definition as it was installed. A template
//! added to the catalog fails [`every_template_of_the_catalog_has_its_journey`]
//! until it has a row.

use super::sealed::{Sealed, AGENT_HARNESS};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::Path;

/// Stands for the project the journey makes, where an input asks for one.
const A_PROJECT: &str = "<a project>";
/// Stands for the workspace's owner, where an input asks for who decides.
const ITS_OWNER: &str = "<its owner>";
/// A command that succeeds and does nothing.
const PASSES: &str = "true";

/// One template's journey.
struct Journey {
    slug: &'static str,
    /// What the goal is for, in its person's words.
    statement: &'static str,
    /// What its person gives it; an input left out takes its default.
    inputs: &'static [(&'static str, &'static str)],
    /// What an agent's result says where the schema alone cannot: a command
    /// a later step runs. `(step, key, value)`.
    says: &'static [(&'static str, &'static str, &'static str)],
    /// The steps its main way passes by; every other step ends `done`.
    passes_by: &'static [&'static str],
    /// The steps that fail and hand the run on, as their `on_fail` says.
    fails: &'static [&'static str],
    /// The steps that open a goal of its own and go on without waiting:
    /// each is followed to its end as its own template's journey.
    opens: &'static [&'static str],
}

/// The standing health check when its check fails: diagnosed, judged
/// severe, and an incident opened — which opens the fix of its cause.
const WHEN_THE_CHECK_FAILS: Journey = Journey {
    slug: "standing-health-check",
    statement: "Watch the service",
    inputs: &[("command", "false"), ("project", A_PROJECT)],
    says: &[],
    passes_by: &["failing", "healthy", "note"],
    fails: &["probe"],
    opens: &["incident"],
};

const JOURNEYS: &[Journey] = &[
    Journey {
        slug: "bug-fix",
        statement: "Fix the login redirect",
        inputs: &[
            ("report", "the login redirect loops"),
            ("project", A_PROJECT),
        ],
        says: &[("reproduce", "command", PASSES)],
        passes_by: &["ask"],
        fails: &[],
        opens: &[],
    },
    Journey {
        slug: "content-pipeline",
        statement: "Write about tea",
        inputs: &[("topic", "tea")],
        says: &[],
        passes_by: &["social"],
        fails: &[],
        opens: &[],
    },
    Journey {
        slug: "customer-support-triage",
        statement: "Triage the export ticket",
        inputs: &[
            ("ticket", "the export fails with a 500"),
            ("project", A_PROJECT),
        ],
        says: &[],
        passes_by: &["ticket", "reply", "finance", "sales", "escalate"],
        fails: &[],
        opens: &["bug"],
    },
    Journey {
        slug: "decision-record",
        statement: "Decide which queue to adopt",
        inputs: &[("decision", "which queue to adopt")],
        says: &[],
        passes_by: &["deferred"],
        fails: &[],
        opens: &[],
    },
    Journey {
        slug: "event-plan",
        statement: "Plan the spring offsite",
        inputs: &[
            ("event", "the spring offsite"),
            ("date", "2027-04-12"),
            ("budget", "20000"),
        ],
        says: &[],
        passes_by: &[],
        fails: &[],
        opens: &[],
    },
    Journey {
        slug: "hiring-loop",
        statement: "Hire a staff engineer",
        inputs: &[("role", "a staff engineer"), ("manager", ITS_OWNER)],
        says: &[],
        passes_by: &["closed"],
        fails: &[],
        opens: &[],
    },
    Journey {
        slug: "incident-response",
        statement: "The API answers 502",
        inputs: &[("symptom", "the API answers 502"), ("project", A_PROJECT)],
        says: &[],
        passes_by: &["run-failed", "call"],
        fails: &[],
        opens: &["followup"],
    },
    Journey {
        slug: "mobile-release",
        statement: "Release 2.4.0",
        inputs: &[
            ("release", "2.4.0"),
            ("project", A_PROJECT),
            ("tests", PASSES),
        ],
        says: &[],
        passes_by: &[],
        fails: &[],
        opens: &[],
    },
    Journey {
        slug: "product-launch",
        statement: "Launch the new editor",
        // Every second, in the six fields that carry seconds: the hold is
        // over at the engine's next look.
        inputs: &[("product", "the new editor"), ("when", "* * * * * *")],
        says: &[],
        passes_by: &[],
        fails: &[],
        opens: &[],
    },
    Journey {
        slug: "research-report",
        statement: "Is tea a meal",
        inputs: &[("question", "is tea a meal")],
        says: &[],
        passes_by: &[],
        fails: &[],
        opens: &[],
    },
    Journey {
        slug: "software-feature",
        statement: "Add a dark mode",
        inputs: &[
            ("feature", "a dark mode"),
            ("project", A_PROJECT),
            ("tests", PASSES),
        ],
        says: &[],
        passes_by: &[],
        fails: &[],
        opens: &[],
    },
    Journey {
        slug: "standing-health-check",
        statement: "Watch the service",
        inputs: &[("command", PASSES), ("project", A_PROJECT)],
        says: &[],
        passes_by: &[
            "failing", "diagnose", "severity", "incident", "note", "done",
        ],
        fails: &[],
        opens: &[],
    },
    Journey {
        slug: "weekly-review",
        statement: "Review the week",
        inputs: &[],
        says: &[],
        passes_by: &["weekly"],
        fails: &[],
        opens: &[],
    },
];

// --- reading a definition ------------------------------------------------------

/// A definition's steps by id, in the order it lists them.
fn steps_of(definition: &Value) -> Vec<(String, Value)> {
    definition["steps"]
        .as_array()
        .expect("a definition has steps")
        .iter()
        .map(|step| {
            (
                step["id"].as_str().expect("a step's id").to_string(),
                step.clone(),
            )
        })
        .collect()
}

/// Where a step leads: `(to, branch)`.
fn leads_to(step: &Value) -> Vec<(String, Option<String>)> {
    step["then"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|edge| {
            (
                edge["to"].as_str().expect("an edge's end").to_string(),
                edge["branch"].as_str().map(str::to_string),
            )
        })
        .collect()
}

/// How many steps from each step to an end that is no failure, along the
/// way a run takes by itself: every branch of a gateway, and of any other
/// step the edges that carry no branch — a branch there is a boundary's.
fn distances(steps: &[(String, Value)]) -> BTreeMap<String, usize> {
    let mut back: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (id, step) in steps {
        let gateway = step["kind"] == "decide";
        for (to, branch) in leads_to(step) {
            if gateway || branch.is_none() {
                back.entry(to).or_default().push(id.clone());
            }
        }
    }
    let mut far: BTreeMap<String, usize> = BTreeMap::new();
    let mut next: VecDeque<String> = steps
        .iter()
        .filter(|(_, step)| step["kind"] == "end" && step["finish"] != "failed")
        .map(|(id, _)| id.clone())
        .collect();
    for id in &next {
        far.insert(id.clone(), 0);
    }
    while let Some(id) = next.pop_front() {
        let here = far[&id];
        for before in back.get(&id).into_iter().flatten() {
            if !far.contains_key(before) {
                far.insert(before.clone(), here + 1);
                next.push_back(before.clone());
            }
        }
    }
    far
}

/// What the main way needs of the agents and of the person: at each
/// gateway the branch that is nearest an end, and what makes it taken.
#[derive(Default)]
struct Steering {
    /// By agent step: what its result says, `key → value`.
    says: BTreeMap<String, Vec<(String, Value)>>,
    /// By agent step: what its result does not say.
    never_says: BTreeMap<String, Vec<(String, Value)>>,
    /// By human step: the option taken.
    takes: BTreeMap<String, String>,
    /// By human step: the options left.
    leaves: BTreeMap<String, BTreeSet<String>>,
}

impl Steering {
    fn of(slug: &str, steps: &[(String, Value)]) -> Self {
        let far = distances(steps);
        let mut steering = Self::default();
        for (id, step) in steps.iter().filter(|(_, step)| step["kind"] == "decide") {
            let nearest = leads_to(step)
                .into_iter()
                .filter_map(|(to, branch)| Some((*far.get(&to)?, branch?)))
                .min_by_key(|(steps, _)| *steps)
                .map(|(_, branch)| branch)
                .unwrap_or_else(|| panic!("{slug}: no branch of `{id}` reaches an end"));
            for rule in step["rules"].as_array().into_iter().flatten() {
                let taken = rule["branch"] == nearest.as_str();
                let when = &rule["when"];
                let of = |key: &str| {
                    when[key]
                        .as_str()
                        .unwrap_or_else(|| panic!("{slug}: `{id}` reads no {key}: {when}"))
                        .to_string()
                };
                match when["condition"].as_str() {
                    Some("output_equals") => {
                        let said = (of("path"), when["value"].clone());
                        let into = if taken {
                            &mut steering.says
                        } else {
                            &mut steering.never_says
                        };
                        into.entry(of("step")).or_default().push(said);
                    }
                    Some("answered") if taken => {
                        steering.takes.insert(of("step"), of("option"));
                    }
                    Some("answered") => {
                        steering
                            .leaves
                            .entry(of("step"))
                            .or_default()
                            .insert(of("option"));
                    }
                    // What its person gave decides; nothing is steered.
                    Some("input_equals") => {}
                    other => panic!(
                        "{slug}: `{id}` decides on {other:?}, which this journey cannot steer — \
                         teach it"
                    ),
                }
                // The first rule that holds is the one taken.
                if taken {
                    break;
                }
            }
        }
        steering
    }

    /// The option a person takes at `step`, when it offers some.
    fn option_at(&self, id: &str, step: &Value) -> Option<String> {
        if let Some(taken) = self.takes.get(id) {
            return Some(taken.clone());
        }
        let left = self.leaves.get(id);
        step["options"]
            .as_array()?
            .iter()
            .filter_map(|option| option["id"].as_str())
            .find(|option| left.is_none_or(|left| !left.contains(*option)))
            .map(str::to_string)
    }
}

/// A value that fits `schema`, named after the key it stands under.
fn fitting(schema: &Value, key: &str) -> Value {
    if let Some(first) = schema["enum"].as_array().and_then(|all| all.first()) {
        return first.clone();
    }
    match schema["type"].as_str() {
        Some("object") => Value::Object(
            schema["properties"]
                .as_object()
                .into_iter()
                .flatten()
                .map(|(name, inner)| (name.clone(), fitting(inner, name)))
                .collect(),
        ),
        Some("array") => json!([fitting(&schema["items"], key)]),
        Some("boolean") => json!(true),
        Some("number" | "integer") => json!(1),
        Some("string") => json!(format!("the {key}, as the script wrote it")),
        other => panic!("a schema of type {other:?} under `{key}`: teach the journey"),
    }
}

/// Another value that fits `schema` and is not `value`.
fn other_than(schema: &Value, value: &Value, key: &str) -> Value {
    if let Some(all) = schema["enum"].as_array() {
        return all
            .iter()
            .find(|member| *member != value)
            .cloned()
            .unwrap_or_else(|| panic!("`{key}` can say nothing but {value}"));
    }
    match value {
        Value::Bool(said) => json!(!said),
        _ => fitting(schema, key),
    }
}

/// What the agent of `step` yields: what its schema asks, what the main way
/// needs of it, and what the journey says by hand.
fn result_of(journey: &Journey, steering: &Steering, id: &str, step: &Value) -> Value {
    let schema = &step["output_schema"];
    let mut result = match fitting(schema, id) {
        Value::Object(result) => result,
        other => panic!("{}: `{id}` yields {other}, not an object", journey.slug),
    };
    let set = |result: &mut Map<String, Value>, key: &str, value: Value| {
        assert!(
            schema["properties"].get(key).is_some(),
            "{}: `{id}` is read for `{key}`, which its schema does not have",
            journey.slug
        );
        result.insert(key.to_string(), value);
    };
    for (key, value) in steering.never_says.get(id).into_iter().flatten() {
        if result.get(key) == Some(value) {
            let other = other_than(&schema["properties"][key], value, key);
            set(&mut result, key, other);
        }
    }
    for (key, value) in steering.says.get(id).into_iter().flatten() {
        set(&mut result, key, value.clone());
    }
    for (_, key, value) in journey.says.iter().filter(|(step, _, _)| *step == id) {
        set(&mut result, key, json!(value));
    }
    Value::Object(result)
}

/// What an agent is told whatever the inputs are: the runs of words of
/// `instructions`, each on one line and between two placeholders.
fn told(instructions: &str) -> Vec<String> {
    instructions
        .lines()
        .flat_map(|line| line.split(['{', '}']).step_by(2))
        .map(|words| words.trim().to_string())
        .filter(|words| !words.is_empty())
        .collect()
}

/// The words a step is known by: the longest it is told that no other agent
/// — of `everybody`, what each is told — is.
fn known_by(id: &str, instructions: &str, everybody: &[String]) -> String {
    told(instructions)
        .into_iter()
        .filter(|words| {
            everybody
                .iter()
                .filter(|other| other.as_str() != instructions)
                .all(|other| !other.contains(words.as_str()))
        })
        .max_by_key(String::len)
        .unwrap_or_else(|| panic!("`{id}` is told nothing another step is not"))
}

/// What every agent step of `definition` is told.
fn instructions_of(definition: &Value) -> Vec<String> {
    steps_of(definition)
        .iter()
        .filter(|(_, step)| step["kind"] == "agent")
        .filter_map(|(_, step)| step["instructions"].as_str().map(str::to_string))
        .collect()
}

/// The turns of every agent step of `definition`, and what each yields.
/// `everybody` is what every agent of the journey is told, this
/// definition's and the others'.
fn turns_of(
    journey: &Journey,
    definition: &Value,
    everybody: &[String],
) -> (Vec<Value>, BTreeMap<String, Value>) {
    let steps = steps_of(definition);
    let steering = Steering::of(journey.slug, &steps);
    let mut turns = Vec::new();
    let mut yields = BTreeMap::new();
    for (id, step) in steps.iter().filter(|(_, step)| step["kind"] == "agent") {
        let words = known_by(
            id,
            step["instructions"].as_str().unwrap_or_default(),
            everybody,
        );
        assert!(
            words.len() > 20,
            "{}: `{id}` tells its agent too little to know it by: {words:?}",
            journey.slug
        );
        let result = result_of(journey, &steering, id, step);
        turns.push(json!({
            "scope": "work_item",
            "when": words,
            "tools": [{ "name": "yield_result", "arguments": { "output": result } }],
            "say": [format!("{id}: done.")],
        }));
        yields.insert(id.clone(), result);
    }
    (turns, yields)
}

// --- the journey ---------------------------------------------------------------

/// A workflow as it was installed, and what its journey makes of it.
struct Installed {
    journey: &'static Journey,
    id: String,
    definition: Value,
    steps: Vec<(String, Value)>,
    steering: Steering,
    /// What each of its agent steps yields.
    yields: BTreeMap<String, Value>,
}

impl Installed {
    fn step(&self, id: &str) -> &Value {
        &self
            .steps
            .iter()
            .find(|(step, _)| step == id)
            .unwrap_or_else(|| {
                panic!(
                    "`{}` has no step `{id}`, which its run has",
                    self.journey.slug
                )
            })
            .1
    }
}

/// Where every step of a run stands, a line each, for a failure to show.
fn where_it_stands(status: &Value) -> String {
    status["run"]["steps"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(id, record)| {
            format!(
                "  {id}: {}{}",
                record["state"],
                record
                    .get("error")
                    .map(|error| format!(" — {error}"))
                    .unwrap_or_default()
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Follow a goal's run to its end: the agents work, the person answers
/// what is asked of them.
fn followed_to_its_end(ws: &Sealed, goal: &str, of: &Installed) -> Value {
    let slug = of.journey.slug;
    let mut seen = Value::Null;
    loop {
        let status = ws.until("the run to move", || {
            let status = ws.json(&["status", goal]);
            (status["run"]["steps"] != seen || status["status"] == "done").then_some(status)
        });
        seen = status["run"]["steps"].clone();
        match status["status"].as_str() {
            Some("done") => return status,
            Some("running" | "waiting" | "queued") => {}
            other => panic!("`{slug}` came to {other:?}:\n{}", where_it_stands(&status)),
        }
        for (id, record) in seen.as_object().into_iter().flatten() {
            if record["state"]["state"] != "waiting" {
                continue;
            }
            let step = of.step(id);
            match step["kind"].as_str() {
                Some("approval") => {
                    ws.ok(&["approve", goal, "--step", id]);
                }
                Some("human") => match of.steering.option_at(id, step) {
                    Some(option) => {
                        ws.ok(&["step", "answer", goal, id, "-o", &option]);
                    }
                    None => {
                        ws.ok(&["step", "answer", goal, id, "as its person sees fit"]);
                    }
                },
                // A hold is over by itself.
                _ => {}
            }
        }
    }
}

/// Every step of the main way is done and said nothing went wrong, and
/// what the run opened went its own main way: a goal for each step that
/// opens one, on the workflow the step names, given what the step says.
fn went_its_main_way(ws: &Sealed, goal: &str, done: &Value, of: &Installed, all: &[Installed]) {
    let journey = of.journey;
    let slug = journey.slug;
    assert_eq!(done["run"]["outcome"], "done", "{}", where_it_stands(done));
    for id in journey.passes_by.iter().chain(journey.fails) {
        of.step(id);
    }
    let mut opened: Vec<(&str, String)> = Vec::new();
    for (id, step) in &of.steps {
        let record = &done["run"]["steps"][id];
        let state = record["state"]["state"].as_str().unwrap_or_default();
        if journey.passes_by.contains(&id.as_str()) {
            assert!(
                matches!(state, "pending" | "skipped"),
                "`{slug}` passes `{id}` by: {record}"
            );
            continue;
        }
        if journey.fails.contains(&id.as_str()) {
            assert_eq!(state, "failed", "`{slug}`, `{id}`: {record}");
            continue;
        }
        assert_eq!(state, "done", "`{slug}`, `{id}`: {record}");
        assert_eq!(record.get("error"), None, "`{slug}`, `{id}`: {record}");
        match step["kind"].as_str() {
            Some("agent") => {
                assert_eq!(record["output"], of.yields[id], "`{slug}`, `{id}`");
                assert_eq!(record["visits"], 1, "`{slug}`, `{id}` was entered once");
            }
            Some("spawn") => opened.push((
                id.as_str(),
                record["output"]["child"]
                    .as_str()
                    .unwrap_or_else(|| panic!("`{slug}`, `{id}` names its child: {record}"))
                    .to_string(),
            )),
            _ => {}
        }
    }
    assert_eq!(
        opened.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
        journey.opens.to_vec(),
        "the goals `{slug}` opened"
    );
    for (id, child) in opened {
        let theirs = all
            .iter()
            .find(|other| of.step(id)["workflow"] == other.id.as_str())
            .unwrap_or_else(|| panic!("`{slug}`, `{id}` opens a goal on a workflow not installed"));
        let born = ws.json(&["status", &child]);
        assert_eq!(
            born["goal"]["origin"],
            json!({ "origin": "spawned", "parent": goal }),
            "`{id}` opened a goal that says whose it is: {born}"
        );
        assert_eq!(born["goal"]["workflow"], theirs.id.as_str(), "{born}");
        let its = followed_to_its_end(ws, &child, theirs);
        // What the step said is what the child was given.
        for (name, said) in of.step(id)["inputs"].as_object().into_iter().flatten() {
            assert!(
                its["run"]["inputs"].get(name).is_some(),
                "`{id}` gives `{name}` ({said}): {}",
                its["run"]["inputs"]
            );
        }
        went_its_main_way(ws, &child, &its, theirs, all);
    }
}

fn installed_and_run(journey: &'static Journey) {
    let slug = journey.slug;
    let mut ws = Sealed::bare();
    ws.start();

    // Installed, through the node, with everything it names.
    let installed = ws.json(&["catalog", "install", "workflow", slug])["installed"].clone();
    let workflows: BTreeMap<String, String> = installed["workflows"]
        .as_array()
        .expect("the workflows installed")
        .iter()
        .filter_map(|pair| Some((pair[0].as_str()?.to_string(), pair[1].as_str()?.to_string())))
        .collect();
    assert!(
        workflows.contains_key(slug),
        "`{slug}` was installed: {installed}"
    );
    let shown: Vec<(&'static Journey, String, Value)> = workflows
        .iter()
        .map(|(other, id)| {
            let theirs = if other == slug {
                journey
            } else {
                JOURNEYS
                    .iter()
                    .find(|journey| journey.slug == other.as_str())
                    .unwrap_or_else(|| panic!("no journey for `{other}`, which `{slug}` names"))
            };
            let shown = ws.json(&["workflow", "show", id]);
            assert_eq!(shown["problems"], json!([]), "`{other}` is installed whole");
            assert_eq!(
                shown["workflow"]["origin"],
                json!({ "origin": "catalog", "slug": other })
            );
            (theirs, id.clone(), shown["workflow"].clone())
        })
        .collect();

    // Its agents, on the harness this machine has.
    let came: BTreeSet<String> = installed["agents"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|agent| agent.as_str().map(str::to_string))
        .collect();
    for (_, _, definition) in &shown {
        for (id, step) in steps_of(definition) {
            if let Some(agent) = step["assignee"]["agent"].as_str() {
                assert!(
                    came.contains(agent),
                    "`{id}` names `{agent}`, which came with nothing: {came:?}"
                );
            }
        }
    }
    for agent in &came {
        ws.ok(&["agent", "edit", agent, "--harness", AGENT_HARNESS]);
    }

    // What the agents do: every agent step of what was installed, the
    // workflows it opens goals on among them.
    let everybody: Vec<String> = shown
        .iter()
        .flat_map(|(_, _, definition)| instructions_of(definition))
        .collect();
    let mut turns = Vec::new();
    let all: Vec<Installed> = shown
        .into_iter()
        .map(|(theirs, id, definition)| {
            let (theirs_turns, yields) = turns_of(theirs, &definition, &everybody);
            turns.extend(theirs_turns);
            let steps = steps_of(&definition);
            Installed {
                journey: theirs,
                id,
                steering: Steering::of(theirs.slug, &steps),
                steps,
                definition,
                yields,
            }
        })
        .collect();
    let mut known = BTreeSet::new();
    for turn in &turns {
        assert!(
            known.insert(turn["when"].as_str().map(str::to_string)),
            "two steps are told the same words: {}",
            turn["when"]
        );
    }
    ws.install_agent(&json!({ "turns": turns }));
    let main = all
        .iter()
        .find(|installed| installed.journey.slug == slug)
        .expect("the template itself");

    // What its person gives it.
    let asked: Vec<&Value> = main.definition["inputs"]
        .as_array()
        .into_iter()
        .flatten()
        .collect();
    let mut given: Vec<String> = Vec::new();
    for (name, value) in journey.inputs {
        let input = asked
            .iter()
            .find(|input| input["name"] == *name)
            .unwrap_or_else(|| panic!("`{slug}` asks for no `{name}`"));
        let value = match *value {
            A_PROJECT => {
                assert_eq!(input["kind"], "project", "{input}");
                let shelf = json!({
                    "kind": "new",
                    "slug": "shelf",
                    "publish": "manual",
                    "git_config": {
                        "user.name": "A Journey",
                        "user.email": "journey@example.test",
                    },
                });
                let (status, made) = ws.call("POST", "/projects", &[], Some(&shelf));
                assert_eq!(status, 200, "{made}");
                made["project"]["id"]
                    .as_str()
                    .expect("the project")
                    .to_string()
            }
            ITS_OWNER => {
                assert_eq!(input["kind"], "assignee", "{input}");
                let (status, workspace) = ws.call("GET", "/workspace", &[], None);
                assert_eq!(status, 200, "{workspace}");
                format!(
                    "human:{}",
                    workspace["pubkey"].as_str().expect("the owner's key")
                )
            }
            said => said.to_string(),
        };
        given.extend(["--input".to_string(), format!("{name}={value}")]);
    }
    for input in &asked {
        let name = input["name"].as_str().unwrap_or_default();
        assert!(
            input["required"] != true || journey.inputs.iter().any(|(given, _)| *given == name),
            "`{slug}` needs `{name}`, which the journey does not give"
        );
    }

    // A goal on it, begun at once — or, where the template begins on events
    // too, captured and begun by hand at its start by hand: given to a goal,
    // such a workflow listens.
    let by_hand: Vec<&str> = main
        .steps
        .iter()
        .filter(|(_, step)| step["kind"] == "start" && step["on"]["event"] == "manual")
        .map(|(id, _)| id.as_str())
        .collect();
    assert_eq!(by_hand.len(), 1, "`{slug}` begins by hand at one step");
    let listens = main
        .steps
        .iter()
        .any(|(_, step)| step["kind"] == "start" && step["on"]["event"] != "manual");
    let mut capture = vec![
        "new",
        journey.statement,
        "--workflow",
        &main.id,
        "--mode",
        "manual",
    ];
    let goal = if listens {
        capture.push("--no-start");
        let goal = ws.json(&capture)["goal"]
            .as_str()
            .expect("the goal")
            .to_string();
        let mut begin = vec!["run", &goal, "--start", by_hand[0]];
        begin.extend(given.iter().map(String::as_str));
        ws.ok(&begin);
        goal
    } else {
        capture.extend(given.iter().map(String::as_str));
        ws.json(&capture)["goal"]
            .as_str()
            .expect("the goal")
            .to_string()
    };

    // To its end, and what it opened to theirs.
    let done = followed_to_its_end(&ws, &goal, main);
    went_its_main_way(&ws, &goal, &done, main, &all);

    // Every agent that was woken has ended, and the run reads the same
    // from its files once the node is gone.
    ws.until("every session to end", || {
        (ws.recorded("ended").len() == ws.recorded("started").len()).then_some(())
    });
    ws.stop();
    let run = done["run"]["id"].as_str().expect("the run");
    let kept = ws.json(&["status", run]);
    assert_eq!(kept["run"]["steps"], done["run"]["steps"]);
}

fn journey_of(slug: &str) -> &'static Journey {
    JOURNEYS
        .iter()
        .find(|journey| journey.slug == slug)
        .unwrap_or_else(|| panic!("no journey for `{slug}`"))
}

#[test]
fn a_standing_health_check_that_fails_opens_an_incident_and_the_fix_of_its_cause() {
    installed_and_run(&WHEN_THE_CHECK_FAILS);
}

macro_rules! journeys {
    ($($test:ident => $slug:literal,)*) => {
        $(
            #[test]
            fn $test() {
                installed_and_run(journey_of($slug));
            }
        )*

        /// The templates that have a journey, as the tests above name them.
        const RUN: &[&str] = &[$($slug,)*];
    };
}

journeys! {
    bug_fix_is_installed_and_runs_to_its_end => "bug-fix",
    content_pipeline_is_installed_and_runs_to_its_end => "content-pipeline",
    customer_support_triage_is_installed_and_runs_to_its_end => "customer-support-triage",
    decision_record_is_installed_and_runs_to_its_end => "decision-record",
    event_plan_is_installed_and_runs_to_its_end => "event-plan",
    hiring_loop_is_installed_and_runs_to_its_end => "hiring-loop",
    incident_response_is_installed_and_runs_to_its_end => "incident-response",
    mobile_release_is_installed_and_runs_to_its_end => "mobile-release",
    product_launch_is_installed_and_runs_to_its_end => "product-launch",
    research_report_is_installed_and_runs_to_its_end => "research-report",
    software_feature_is_installed_and_runs_to_its_end => "software-feature",
    standing_health_check_is_installed_and_runs_to_its_end => "standing-health-check",
    weekly_review_is_installed_and_runs_to_its_end => "weekly-review",
}

#[test]
fn every_template_of_the_catalog_has_its_journey() {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../library/catalog/workflows");
    let mut shipped: Vec<String> = std::fs::read_dir(&folder)
        .unwrap_or_else(|e| panic!("the catalog's workflows at {}: {e}", folder.display()))
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            name.strip_suffix(".toml").map(str::to_string)
        })
        .collect();
    shipped.sort();
    assert!(shipped.len() >= 13, "the catalog was read: {shipped:?}");
    let mut run: Vec<String> = RUN.iter().map(|slug| slug.to_string()).collect();
    run.sort();
    assert_eq!(
        run, shipped,
        "a template with no journey, or a journey with no template"
    );
    let mut rows: Vec<String> = JOURNEYS.iter().map(|j| j.slug.to_string()).collect();
    rows.sort();
    assert_eq!(rows, shipped, "and each has its row");
}
