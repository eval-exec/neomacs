//! Shared result protocol for differential GNU Emacs/Neomacs tests.
//!
//! Editor-specific sandboxes remain adapters owned by their test crates. This
//! crate defines the small common interface at the comparison seam: an
//! evaluation either returns a normalized printed Lisp value or signals
//! normalized printed Lisp error data.

use std::fmt;

/// A normalized, comparable editor evaluation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EvalOutcome {
    Value(String),
    Signal(String),
}

/// The protocol variant a parity case is required to produce.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExpectedOutcome {
    Value,
    Signal,
}

impl ExpectedOutcome {
    pub fn matches(self, outcome: &EvalOutcome) -> bool {
        matches!(
            (self, outcome),
            (Self::Value, EvalOutcome::Value(_)) | (Self::Signal, EvalOutcome::Signal(_))
        )
    }
}

impl EvalOutcome {
    /// Parse the `OK …` / `ERR …` protocol emitted by an editor adapter.
    pub fn parse(encoded: &str) -> Result<Self, String> {
        let encoded = encoded.trim();
        if let Some(value) = encoded.strip_prefix("OK ") {
            return Ok(Self::Value(value.to_string()));
        }
        if let Some(signal) = encoded.strip_prefix("ERR ") {
            return Ok(Self::Signal(signal.to_string()));
        }
        Err(format!(
            "expected an oracle outcome beginning with `OK ` or `ERR `, got `{encoded}`"
        ))
    }

    pub fn is_value(&self) -> bool {
        matches!(self, Self::Value(_))
    }
}

impl fmt::Display for EvalOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Value(value) => write!(formatter, "OK {value}"),
            Self::Signal(signal) => write!(formatter, "ERR {signal}"),
        }
    }
}

/// One editor evaluation with ordinary stdout kept separate from its typed
/// result.
///
/// Editor probes are allowed to print arbitrary bytes to stdout.  Their final
/// `EvalOutcome` therefore travels over the marked debugging-output protocol
/// instead of sharing stdout with the code under test.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapturedEvaluation {
    stdout: String,
    outcome: EvalOutcome,
}

impl CapturedEvaluation {
    /// Decode an evaluation from the editor's separately captured streams.
    pub fn from_marked_streams(
        stdout: &str,
        debugging_output: &str,
        marker: &str,
    ) -> Result<Self, String> {
        Ok(Self {
            stdout: stdout.to_string(),
            outcome: extract_marked_outcome(debugging_output, marker)?,
        })
    }

    /// Ordinary stdout emitted by the evaluated form.
    pub fn stdout(&self) -> &str {
        &self.stdout
    }

    /// The normalized value or signal emitted by the result protocol.
    pub fn outcome(&self) -> &EvalOutcome {
        &self.outcome
    }

    /// Render the historical oracle snapshot format: ordinary stdout followed
    /// immediately by the encoded outcome, with outer whitespace trimmed.
    pub fn legacy_transcript(&self) -> String {
        format!("{}{outcome}", self.stdout, outcome = self.outcome)
            .trim()
            .to_string()
    }
}

/// Extract the last marked outcome from a noisy editor output stream.
pub fn extract_marked_outcome(output: &str, marker: &str) -> Result<EvalOutcome, String> {
    let encoded = output
        .lines()
        .filter_map(|line| line.split_once(marker).map(|(_, encoded)| encoded.trim()))
        .next_back()
        .ok_or_else(|| format!("editor output did not contain outcome marker `{marker}`"))?;
    EvalOutcome::parse(encoded)
}

/// One case id paired with its parsed editor outcome.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MarkedBatchOutcome {
    pub id: String,
    pub outcome: EvalOutcome,
}

fn marked_batch_outcome(line: &str, marker: &str) -> Result<Option<MarkedBatchOutcome>, String> {
    let Some(rest) = line.strip_prefix(marker).map(str::trim) else {
        return Ok(None);
    };
    let Some((id, encoded)) = rest.split_once(':') else {
        return Err(format!(
            "editor output contained malformed batch outcome record `{rest}` after `{marker}`"
        ));
    };
    let id = id.trim();
    validate_batch_case_id(id).map_err(|error| {
        format!("editor output contained malformed batch outcome id `{id}`: {error}")
    })?;
    Ok(Some(MarkedBatchOutcome {
        id: id.to_string(),
        outcome: EvalOutcome::parse(encoded.trim())?,
    }))
}

/// One strictly ordered batch protocol stream.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MarkedBatchProtocol {
    /// Case ids in the order their `BEGIN` records appeared.
    pub case_ids: Vec<String>,
    /// Outcomes in the same order, one between each case's begin and complete.
    pub outcomes: Vec<MarkedBatchOutcome>,
    /// The case that began but did not complete, if the stream ended mid-case.
    pub unfinished_case_id: Option<String>,
}

/// Parse `BEGIN -> outcome -> COMPLETE` records from one ordered stream.
///
/// The final case may be unfinished so timeout diagnostics can identify it.
/// Every other ordering, duplicate, malformed, or mismatched record is an
/// infrastructure error.
pub fn extract_marked_batch_protocol(
    output: &str,
    begin_marker: &str,
    outcome_marker: &str,
    completion_marker: &str,
) -> Result<MarkedBatchProtocol, String> {
    let mut case_ids = Vec::new();
    let mut outcomes = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut active: Option<(String, bool)> = None;

    for line in output.lines() {
        let begin = marked_batch_case_id(line, begin_marker, "begin")?;
        let outcome = marked_batch_outcome(line, outcome_marker)?;
        let completion = marked_batch_case_id(line, completion_marker, "completion")?;
        let record_count = usize::from(begin.is_some())
            + usize::from(outcome.is_some())
            + usize::from(completion.is_some());
        if record_count > 1 {
            return Err(format!(
                "editor output line contained multiple batch protocol records: `{line}`"
            ));
        }

        if let Some(id) = begin {
            if let Some((current, _)) = active.as_ref() {
                return Err(format!(
                    "batch case `{id}` began before active case `{current}` completed"
                ));
            }
            if !seen.insert(id.clone()) {
                return Err(format!(
                    "editor output contained duplicate batch begin id `{id}`"
                ));
            }
            case_ids.push(id.clone());
            active = Some((id, false));
        } else if let Some(outcome) = outcome {
            let Some((current, has_outcome)) = active.as_mut() else {
                return Err(format!(
                    "batch case `{}` emitted an outcome without a matching active begin record",
                    outcome.id
                ));
            };
            if outcome.id != *current {
                return Err(format!(
                    "batch case `{}` emitted an outcome while active case `{current}` was unfinished",
                    outcome.id
                ));
            }
            if *has_outcome {
                return Err(format!(
                    "batch case `{current}` emitted more than one outcome record"
                ));
            }
            *has_outcome = true;
            outcomes.push(outcome);
        } else if let Some(id) = completion {
            let Some((current, has_outcome)) = active.take() else {
                return Err(format!(
                    "batch case `{id}` completed without a matching active begin record"
                ));
            };
            if id != current {
                return Err(format!(
                    "batch case `{id}` completed while active case `{current}` was unfinished"
                ));
            }
            if !has_outcome {
                return Err(format!(
                    "batch case `{id}` completed before emitting its outcome"
                ));
            }
        }
    }

    Ok(MarkedBatchProtocol {
        case_ids,
        outcomes,
        unfinished_case_id: active.map(|(id, _)| id),
    })
}

fn marked_batch_case_id(
    line: &str,
    marker: &str,
    record_kind: &str,
) -> Result<Option<String>, String> {
    let Some(rest) = line.strip_prefix(marker).map(str::trim) else {
        return Ok(None);
    };
    validate_batch_case_id(rest).map_err(|error| {
        format!("editor output contained malformed batch {record_kind} record `{rest}`: {error}")
    })?;
    Ok(Some(rest.to_string()))
}

/// Lisp that defines the shared normalizer used by every oracle transport.
pub fn oracle_normalizer_elisp() -> &'static str {
    r##"(defun neomacs--test-oracle-normalize-string (value)
             (dolist
                 (root
                  (list
                   (cons (getenv "HOME") "[ORACLE-HOME]")
                   (cons (getenv "TMPDIR") "[ORACLE-TMPDIR]")
                   (cons (getenv "XDG_CONFIG_HOME") "[ORACLE-XDG-CONFIG]")
                   (cons (getenv "XDG_CACHE_HOME") "[ORACLE-XDG-CACHE]")
                   (cons (getenv "XDG_DATA_HOME") "[ORACLE-XDG-DATA]")
                   (cons (getenv "XDG_STATE_HOME") "[ORACLE-XDG-STATE]")
                   (cons (getenv "NEOMACS_TEST_SANDBOX_ROOT")
                         "[ORACLE-SANDBOX]")
                   (cons (getenv "NEOMACS_TEST_WORKSPACE_ROOT")
                         "[ORACLE-WORKSPACE]")))
               (when (and (stringp (car root))
                          (> (length (car root)) 1))
                 (setq value
                       (replace-regexp-in-string
                        (regexp-quote
                         (directory-file-name (car root)))
                        (cdr root)
                        value t t))))
             value)
           (defun neomacs--test-oracle-normalize (value seen)
             (cond
              ((stringp value)
               (neomacs--test-oracle-normalize-string value))
              ;; Some Neomacs runtime handles currently use integer IDs, so
              ;; predicates such as `windowp' can also accept ordinary small
              ;; integers. Preserve numeric values before probing opaque
              ;; runtime object predicates.
              ((numberp value) value)
              ((and (fboundp 'bufferp) (bufferp value))
               (list :buffer (buffer-name value)))
              ((and (fboundp 'markerp) (markerp value))
               (list :marker
                     (marker-position value)
                     (let ((buffer (marker-buffer value)))
                       (and buffer (buffer-name buffer)))))
              ((and (fboundp 'processp) (processp value))
               (list :process
                     (process-name value)
                     (process-status value)))
              ((and (fboundp 'windowp) (windowp value))
               (list :window
                     (let ((buffer (window-buffer value)))
                       (and buffer (buffer-name buffer)))))
              ((and (fboundp 'framep) (framep value))
               (list :frame
                     (frame-parameter value 'name)))
              ((consp value)
               ;; Walk the cdr chain iteratively.  This function's recursion
               ;; depth must track how deeply a value nests, not how long a
               ;; list is: recursing on the cdr cost one frame per cons, so a
               ;; flat list of 316 elements exhausted `max-lisp-eval-depth'
               ;; and the overflow surfaced from inside the oracle's own
               ;; error handler, indistinguishable from the package under
               ;; test signalling.  Each cons is still registered in SEEN
               ;; before its car is normalized, so shared structure and
               ;; cycles resolve exactly as before.
               (or (gethash value seen)
                   (let* ((copy (cons nil nil))
                          (tail copy)
                          (rest (cdr value)))
                     (puthash value copy seen)
                     (setcar
                      copy
                      (neomacs--test-oracle-normalize (car value) seen))
                     (while (and (consp rest) (not (gethash rest seen)))
                       (let ((next (cons nil nil)))
                         (puthash rest next seen)
                         (setcar
                          next
                          (neomacs--test-oracle-normalize (car rest) seen))
                         (setcdr tail next)
                         (setq tail next
                               rest (cdr rest))))
                     (setcdr
                      tail
                      (neomacs--test-oracle-normalize rest seen))
                     copy)))
              ((vectorp value)
               (or (gethash value seen)
                   (let* ((length (length value))
                          (copy (make-vector length nil)))
                     (puthash value copy seen)
                     (dotimes (index length)
                       (aset
                        copy index
                        (neomacs--test-oracle-normalize
                         (aref value index) seen)))
                     copy)))
              (t value)))
           (defun neomacs--test-oracle-normalized (value)
             (neomacs--test-oracle-normalize
              value
              (make-hash-table :test 'eq)))"##
}

/// Wrap setup and probe forms in the shared result protocol.
///
/// Both inputs may contain multiple forms. The value of the probe's final
/// form is recursively normalized and printed with `prin1`; ordinary Lisp
/// errors are caught and their complete signal data is normalized and printed
/// instead. The marked result is written to `external-debugging-output`, so
/// ordinary stdout remains data owned by the evaluated form. Workspace and
/// per-engine sandbox roots come from the
/// `NEOMACS_TEST_WORKSPACE_ROOT` and `NEOMACS_TEST_SANDBOX_ROOT` environment
/// variables.
pub fn wrap_elisp_outcome(setup: &str, probe: &str, marker: &str) -> String {
    let marker = elisp_string(marker);
    format!(
        r##"(let ((print-circle t)
                  (print-length nil)
                  (print-level nil)
                  (print-escape-newlines t)
                  (print-escape-control-characters t))
           {normalizer}
           (condition-case neomacs--oracle-error
               (let ((neomacs--oracle-result
                      (progn
                        {setup}
                        {probe})))
                 (princ "\n" 'external-debugging-output)
                 (princ {marker} 'external-debugging-output)
                 (princ "OK " 'external-debugging-output)
                 (prin1
                  (neomacs--test-oracle-normalized
                   neomacs--oracle-result)
                  'external-debugging-output)
                 (terpri 'external-debugging-output))
             (error
              (princ "\n" 'external-debugging-output)
              (princ {marker} 'external-debugging-output)
              (princ "ERR " 'external-debugging-output)
              (prin1
               (neomacs--test-oracle-normalized
                neomacs--oracle-error)
               'external-debugging-output)
              (terpri 'external-debugging-output))))"##,
        normalizer = oracle_normalizer_elisp(),
        setup = setup,
        probe = probe,
        marker = marker,
    )
}

/// One named probe embedded in a multi-probe batch process.
#[derive(Clone, Copy, Debug)]
pub struct BatchProbe<'a> {
    /// Stable case id. Must be non-empty and must not contain `:`.
    pub id: &'a str,
    /// Elisp forms evaluated after shared setup; final value is the outcome.
    pub probe: &'a str,
}

/// Wrap shared setup plus many named probes for one editor process.
///
/// Setup runs once. Each probe is wrapped in its own `condition-case` so a
/// signal in one case does not stop later cases. Emitted lines look like:
///
/// ```text
/// <marker><id>:OK …
/// <marker><id>:ERR …
/// ```
pub fn wrap_elisp_batch_outcomes(
    setup: &str,
    cases: &[BatchProbe<'_>],
    begin_marker: &str,
    completion_marker: &str,
    outcome_marker: &str,
) -> Result<String, String> {
    if cases.is_empty() {
        return Err("batch outcomes require at least one probe".into());
    }
    let begin_marker_lit = elisp_string(begin_marker);
    let completion_marker_lit = elisp_string(completion_marker);
    let outcome_marker_lit = elisp_string(outcome_marker);
    let mut body = String::new();
    body.push_str(setup);
    body.push('\n');
    let mut seen = std::collections::HashSet::new();
    for case in cases {
        validate_batch_case_id(case.id)?;
        if !seen.insert(case.id) {
            return Err(format!("duplicate batch case id `{}`", case.id));
        }
        let id_lit = elisp_string(case.id);
        body.push_str(&format!(
            r##"
           (neomacs--test-oracle-case-begin {id})
           (condition-case neomacs--oracle-error
               (let ((neomacs--oracle-result
                      (progn
                        {probe})))
                 (princ "\n" 'external-debugging-output)
                 (princ {marker} 'external-debugging-output)
                 (princ {id} 'external-debugging-output)
                 (princ ":" 'external-debugging-output)
                 (princ "OK " 'external-debugging-output)
                 (prin1
                  (neomacs--test-oracle-normalized
                   neomacs--oracle-result)
                  'external-debugging-output)
                 (terpri 'external-debugging-output))
             (error
              (princ "\n" 'external-debugging-output)
              (princ {marker} 'external-debugging-output)
              (princ {id} 'external-debugging-output)
              (princ ":" 'external-debugging-output)
              (princ "ERR " 'external-debugging-output)
              (prin1
               (neomacs--test-oracle-normalized
                neomacs--oracle-error)
               'external-debugging-output)
              (terpri 'external-debugging-output)))
           (neomacs--test-oracle-case-complete {id})
"##,
            probe = case.probe,
            marker = outcome_marker_lit,
            id = id_lit,
        ));
    }

    Ok(format!(
        r##"(let ((print-circle t)
                  (print-length nil)
                  (print-level nil)
                  (print-escape-newlines t)
                  (print-escape-control-characters t))
           {normalizer}
           (defun neomacs--test-oracle-case-begin (id)
             (princ {begin_marker} 'external-debugging-output)
             (princ id 'external-debugging-output)
             (terpri 'external-debugging-output))
           (defun neomacs--test-oracle-case-complete (id)
             (princ {completion_marker} 'external-debugging-output)
             (princ id 'external-debugging-output)
             (terpri 'external-debugging-output))
           (progn
             {body}))"##,
        normalizer = oracle_normalizer_elisp(),
        begin_marker = begin_marker_lit,
        completion_marker = completion_marker_lit,
        body = body,
    ))
}

/// Reject empty ids and ids that would break the `MARKER<id>:` wire format.
pub fn validate_batch_case_id(id: &str) -> Result<(), String> {
    if id.is_empty() {
        return Err("batch case id must not be empty".into());
    }
    if id.contains(':') {
        return Err(format!(
            "batch case id `{id}` must not contain ':' (reserved by the batch outcome protocol)"
        ));
    }
    if id.chars().any(|c| c.is_whitespace()) {
        return Err(format!("batch case id `{id}` must not contain whitespace"));
    }
    Ok(())
}

fn elisp_string(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}
