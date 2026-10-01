//! Questions and answers between an agent and a person.
//!
//! Two rules about power, not data. **Free text and "I am not sure" are valid
//! on every question, whatever the asker offered**: an agent choosing what to
//! offer must not be able to choose what a person is allowed to say. And **an
//! unknown option id is refused, never dropped**: a selection that quietly
//! resolves to nothing is an answer the human believes they gave.

use serde::{Deserialize, Serialize};

/// One offered answer to a question.
///
/// `id` is the token the human's answer comes back as; `label` is what they
/// read. They are separate so that rewording a question after it was asked
/// cannot change what a recorded answer meant.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AskOption {
    /// Stable token. Answers name this, never the label.
    pub id: String,
    /// What the human reads.
    pub label: String,
    /// One line of "what this actually costs you", shown under the label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// The asker's own recommendation. At most one option per question may
    /// set it — two recommendations is not advice.
    #[serde(default)]
    pub recommended: bool,
}

impl AskOption {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            detail: None,
            recommended: false,
        }
    }

    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn recommend(mut self) -> Self {
        self.recommended = true;
        self
    }
}

/// What kind of human input a question expects.
///
/// The option list lives *inside* the `Answer` variant rather than beside the
/// enum. A sibling `options` field would let `expects: Decision` carry a
/// populated option list — a state with no meaning that every reader would
/// have to decide how to ignore.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "snake_case", tag = "kind")]
pub enum AskKind {
    /// Approve or decline. The gates.
    Decision,
    /// The human tells you something. `options` may be empty — a free-text
    /// question is still the right shape when the answer is not enumerable.
    Answer {
        #[serde(default)]
        options: Vec<AskOption>,
        /// Whether more than one option may be chosen.
        #[serde(default)]
        multi: bool,
    },
}

impl AskKind {
    /// A question with no options.
    pub fn free_text() -> Self {
        AskKind::Answer {
            options: vec![],
            multi: false,
        }
    }

    /// A single-choice question over `options`.
    pub fn one_of(options: Vec<AskOption>) -> Self {
        AskKind::Answer {
            options,
            multi: false,
        }
    }

    /// A multi-choice question over `options`.
    pub fn any_of(options: Vec<AskOption>) -> Self {
        AskKind::Answer {
            options,
            multi: true,
        }
    }

    pub fn options(&self) -> &[AskOption] {
        match self {
            AskKind::Decision => &[],
            AskKind::Answer { options, .. } => options,
        }
    }

    pub fn is_answer(&self) -> bool {
        matches!(self, AskKind::Answer { .. })
    }

    /// Check the question's own shape, before it is put in front of a human.
    pub fn validate(&self) -> Result<(), AnswerError> {
        let options = self.options();
        if options.iter().filter(|o| o.recommended).count() > 1 {
            return Err(AnswerError::ManyRecommended);
        }
        let mut seen: Vec<&str> = Vec::with_capacity(options.len());
        for o in options {
            if o.id.trim().is_empty() {
                return Err(AnswerError::EmptyOptionId);
            }
            if seen.contains(&o.id.as_str()) {
                return Err(AnswerError::DuplicateOptionId(o.id.clone()));
            }
            seen.push(&o.id);
        }
        Ok(())
    }

    /// Check an answer against the question that was actually asked.
    pub fn validate_answer(&self, answer: Option<&Answer>) -> Result<(), AnswerError> {
        match self {
            // A decision gate takes a verdict and, optionally, a reason. There
            // is no third door on a gate: not knowing whether to approve is a
            // reason not to answer yet.
            AskKind::Decision => match answer {
                Some(a) if !a.selected.is_empty() => Err(AnswerError::SelectionOnDecision),
                Some(a) if a.unsure => Err(AnswerError::UnsureOnDecision),
                _ => Ok(()),
            },
            AskKind::Answer { options, multi } => {
                let Some(a) = answer else {
                    return Err(AnswerError::Empty);
                };
                if a.is_empty() {
                    return Err(AnswerError::Empty);
                }
                if !*multi && a.selected.len() > 1 {
                    return Err(AnswerError::MultiNotOffered);
                }
                for id in &a.selected {
                    if !options.iter().any(|o| &o.id == id) {
                        return Err(AnswerError::UnknownOption(id.clone()));
                    }
                }
                Ok(())
            }
        }
    }
}

/// What a human said back. One value that can say all three things at once.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Answer {
    /// Option ids chosen. Empty when the question offered none.
    #[serde(default)]
    pub selected: Vec<String>,
    /// What they typed — alongside the options, not instead of them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// They do not know. Not a decline.
    #[serde(default)]
    pub unsure: bool,
}

impl Answer {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            selected: vec![],
            text: Some(text.into()),
            unsure: false,
        }
    }

    pub fn selecting<I, S>(ids: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            selected: ids.into_iter().map(Into::into).collect(),
            text: None,
            unsure: false,
        }
    }

    pub fn unsure() -> Self {
        Self {
            selected: vec![],
            text: None,
            unsure: true,
        }
    }

    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }

    /// Nothing was actually said. Whitespace is nothing.
    pub fn is_empty(&self) -> bool {
        self.selected.is_empty()
            && !self.unsure
            && self.text.as_deref().map(str::trim).unwrap_or("").is_empty()
    }
}

/// Why a question or an answer was refused at the boundary.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum AnswerError {
    #[error(
        "an answer must carry a selection, some text, or \"I'm not sure\" — this carried nothing"
    )]
    Empty,
    #[error("a decision gate is approve/decline; it offers no options to select")]
    SelectionOnDecision,
    #[error(
        "a decision gate is approve/decline — leave it pending rather than \
         approving it while unsure"
    )]
    UnsureOnDecision,
    #[error("option {0:?} was never offered by this question")]
    UnknownOption(String),
    #[error("this question takes one answer, not several")]
    MultiNotOffered,
    #[error("a question may recommend at most one option")]
    ManyRecommended,
    #[error("option ids must be non-empty")]
    EmptyOptionId,
    #[error("option id {0:?} appears twice")]
    DuplicateOptionId(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_decision_gate_refuses_i_am_not_sure_and_selections() {
        let gate = AskKind::Decision;
        assert!(matches!(
            gate.validate_answer(Some(&Answer::unsure())),
            Err(AnswerError::UnsureOnDecision)
        ));
        assert!(gate.validate_answer(None).is_ok());
        assert!(matches!(
            gate.validate_answer(Some(&Answer::selecting(["a"]))),
            Err(AnswerError::SelectionOnDecision)
        ));
    }

    #[test]
    fn a_question_accepts_i_am_not_sure_and_free_text_whatever_it_offered() {
        let q = AskKind::one_of(vec![AskOption::new("a", "A")]);
        assert!(q.validate_answer(Some(&Answer::unsure())).is_ok());
        assert!(q
            .validate_answer(Some(&Answer::text("something else")))
            .is_ok());
        assert!(AskKind::free_text()
            .validate_answer(Some(&Answer::unsure()))
            .is_ok());
    }

    #[test]
    fn unknown_and_multiple_selections_are_refused() {
        let q = AskKind::one_of(vec![AskOption::new("a", "A"), AskOption::new("b", "B")]);
        assert_eq!(
            q.validate_answer(Some(&Answer::selecting(["zzz"]))),
            Err(AnswerError::UnknownOption("zzz".into()))
        );
        assert_eq!(
            q.validate_answer(Some(&Answer::selecting(["a", "b"]))),
            Err(AnswerError::MultiNotOffered)
        );
        assert!(
            AskKind::any_of(vec![AskOption::new("a", "A"), AskOption::new("b", "B")])
                .validate_answer(Some(&Answer::selecting(["a", "b"])))
                .is_ok()
        );
    }

    #[test]
    fn empty_answers_are_nothing() {
        assert!(Answer::default().is_empty());
        assert!(Answer::text("   ").is_empty());
        assert!(!Answer::unsure().is_empty());
        assert_eq!(
            AskKind::free_text().validate_answer(Some(&Answer::text(" "))),
            Err(AnswerError::Empty)
        );
    }

    #[test]
    fn the_questions_own_shape_is_checked() {
        assert_eq!(
            AskKind::one_of(vec![
                AskOption::new("a", "A").recommend(),
                AskOption::new("b", "B").recommend()
            ])
            .validate(),
            Err(AnswerError::ManyRecommended)
        );
        assert_eq!(
            AskKind::one_of(vec![AskOption::new("a", "A"), AskOption::new("a", "A2")]).validate(),
            Err(AnswerError::DuplicateOptionId("a".into()))
        );
        assert_eq!(
            AskKind::one_of(vec![AskOption::new(" ", "A")]).validate(),
            Err(AnswerError::EmptyOptionId)
        );
        assert!(AskKind::Decision.validate().is_ok());
    }
}
