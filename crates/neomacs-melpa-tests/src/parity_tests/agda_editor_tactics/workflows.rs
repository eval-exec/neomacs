use expect_test::expect;

use super::ParityBatchCase;

fn agda_editor_tactics_documented_record_parses_and_renders_end_to_end() -> ParityBatchCase {
    ParityBatchCase::value(
        "agda_editor_tactics_documented_record_parses_and_renders_end_to_end",
        r##"(let* ((source
          "record R (A : Set) (n : ℕ) : Set where
  field
    x : A
    v : Vec A n")
        (parsed (agda-editor-tactics-record-info source))
        (rendered (agda-editor-tactics-as-Σ-nested parsed)))
         (list source parsed rendered))"##,
        expect![[
            r#"OK ("record R (A : Set) (n : ℕ) : Set where\n  field\n    x : A\n    v : Vec A n" (:name "R" :level "" :body ((:param "A : Set") (:param "n : ℕ") (:field "x : A") (:field "v : Vec A n"))) "R′ : (A : Set) (n : ℕ) → Set \nR′ = λ A n → Σ x ∶ A • Σ v ∶ Vec A n • ⊤")"#
        ]],
    )
}

fn agda_editor_tactics_realistic_monoid_record_preserves_computed_locals() -> ParityBatchCase {
    ParityBatchCase::value(
        "agda_editor_tactics_realistic_monoid_record_preserves_computed_locals",
        r##"(let* ((source
          "record Monoid (ℓ : Level) : Set (lsuc ℓ) where
  Carrier : Set ℓ
  field
    ε : Carrier
    _∙_ : Carrier → Carrier → Carrier
  twice : Carrier → Carrier
  twice x = x ∙ x
  field
    left-identity : ∀ x → ε ∙ x ≡ x
    right-identity : ∀ x → x ∙ ε ≡ x")
        (parsed (agda-editor-tactics-record-info source)))
         (list
          (plist-get parsed :name)
          (plist-get parsed :level)
          (plist-get parsed :body)
          (agda-editor-tactics-as-Σ-nested parsed)))"##,
        expect![[
            r#"OK ("Monoid" " (lsuc ℓ)" ((:param "ℓ : Level") (:local "Carrier : Set ℓ") (:field "ε : Carrier") (:field "_∙_ : Carrier → Carrier → Carrier") (:local "twice : Carrier → Carrier") (:local "twice x = x ∙ x") (:field "left-identity : ∀ x → ε ∙ x ≡ x") (:field "right-identity : ∀ x → x ∙ ε ≡ x")) "Monoid′ : (ℓ : Level) → Set  (lsuc ℓ)\nMonoid′ = λ ℓ → let Carrier : Set ℓ in Σ ε ∶ Carrier • Σ _∙_ ∶ Carrier → Carrier → Carrier • let twice : Carrier → Carrier ; twice x = x ∙ x in Σ left-identity ∶ ∀ x → ε ∙ x ≡ x • Σ right-identity ∶ ∀ x → x ∙ ε ≡ x • ⊤")"#
        ]],
    )
}

fn agda_editor_tactics_dependent_family_workflow_keeps_binder_dependencies() -> ParityBatchCase {
    ParityBatchCase::value(
        "agda_editor_tactics_dependent_family_workflow_keeps_binder_dependencies",
        r##"(let* ((source
          "record Displayed (o h : Level) (Obj : Set o) : Set (o ⊔ lsuc h) where
  field
    Hom : Obj → Obj → Set h
    id : (A : Obj) → Hom A A
  compose-type : Set (o ⊔ h)
  compose-type = (A B C : Obj) → Hom B C → Hom A B → Hom A C
  field
    compose : compose-type")
        (parsed (agda-editor-tactics-record-info source))
        (rendered (agda-editor-tactics-as-Σ-nested parsed)))
         (list
          (mapcar #'cadr (plist-get parsed :body))
          rendered
          (length rendered)))"##,
        expect![[
            r#"OK (("o h : Level" "Obj : Set o" "Hom : Obj → Obj → Set h" "id : (A : Obj) → Hom A A" "compose-type : Set (o ⊔ h)" "compose-type = (A B C : Obj) → Hom B C → Hom A B → Hom A C" "compose : compose-type") "Displayed′ : (o h : Level) (Obj : Set o) → Set  (o ⊔ lsuc h)\nDisplayed′ = λ o h Obj → Σ Hom ∶ Obj → Obj → Set h • Σ id ∶ (A ∶ Obj) → Hom A A • let compose-type : Set (o ⊔ h) ; compose-type = (A B C : Obj) → Hom B C → Hom A B → Hom A C in Σ compose ∶ compose-type • ⊤" 266)"#
        ]],
    )
}

fn agda_editor_tactics_custom_naming_applies_to_declaration_and_definition() -> ParityBatchCase {
    ParityBatchCase::value(
        "agda_editor_tactics_custom_naming_applies_to_declaration_and_definition",
        r##"(let* ((agda-editor-tactics-format-Σ-naming "%s-SigmaView")
        (source
         "record PairView (A B : Set) : Set where
  field
    first : A
    second : B")
        (parsed (agda-editor-tactics-record-info source))
        (rendered (agda-editor-tactics-as-Σ-nested parsed)))
         (list
          agda-editor-tactics-format-Σ-naming
          parsed
          rendered
          (string-match-p "PairView-SigmaView : " rendered)
          (string-match-p "\nPairView-SigmaView = " rendered)))"##,
        expect![[
            r#"OK ("%s-SigmaView" (:name "PairView" :level "" :body ((:param "A B : Set") (:field "first : A") (:field "second : B"))) "PairView-SigmaView : (A B : Set) → Set \nPairView-SigmaView = λ A B → Σ first ∶ A • Σ second ∶ B • ⊤" 0 39)"#
        ]],
    )
}

fn agda_editor_tactics_transforms_a_batch_of_distinct_record_shapes() -> ParityBatchCase {
    ParityBatchCase::value(
        "agda_editor_tactics_transforms_a_batch_of_distinct_record_shapes",
        r##"(mapcar
         (lambda (source)
           (let ((parsed (agda-editor-tactics-record-info source)))
             (list
              (plist-get parsed :name)
              (length (plist-get parsed :body))
              (agda-editor-tactics-as-Σ-nested parsed))))
         '("record Point : Set where
  field
    x : ℕ
    y : ℕ"
           "record Wrapper (A : Set) : Set where
  field
    unwrap : A"
           "record Stateful (S : Set) : Set where
  initial : S
  initial = default
  field
    step : S → S
  reached : S
  reached = step initial"))"##,
        expect![[
            r#"OK (("Point" 2 "Point′ : Set \nPoint′ = Σ x ∶ ℕ • Σ y ∶ ℕ • ⊤") ("Wrapper" 2 "Wrapper′ : (A : Set) → Set \nWrapper′ = λ A → Σ unwrap ∶ A • ⊤") ("Stateful" 6 "Stateful′ : (S : Set) → Set \nStateful′ = λ S → let initial : S  initial = default in Σ step ∶ S → S • let reached : S ; reached = step initial in ⊤"))"#
        ]],
    )
}

fn agda_editor_tactics_editor_buffer_workflow_replaces_a_selected_record() -> ParityBatchCase {
    ParityBatchCase::value(
        "agda_editor_tactics_editor_buffer_workflow_replaces_a_selected_record",
        r##"(with-temp-buffer
         (insert
          "-- preceding Agda code\n"
          "record BufferRecord (A : Set) : Set where\n"
          "  helper : A → A\n"
          "  helper x = x\n"
          "  field\n"
          "    value : A\n"
          "    law : helper value ≡ value\n"
          "-- following Agda code\n")
         (goto-char (point-min))
         (search-forward "record BufferRecord")
         (beginning-of-line)
         (let ((start (point)))
           (search-forward "-- following")
           (beginning-of-line)
           (let* ((end (point))
                  (record-text (buffer-substring-no-properties start end))
                  (replacement
                   (agda-editor-tactics-as-Σ-nested
                    (agda-editor-tactics-record-info record-text))))
             (delete-region start end)
             (goto-char start)
             (insert replacement "\n")
             (list
              record-text
              replacement
              (buffer-substring-no-properties (point-min) (point-max))
              (line-number-at-pos)))))"##,
        expect![[
            r#"OK ("record BufferRecord (A : Set) : Set where\n  helper : A → A\n  helper x = x\n  field\n    value : A\n    law : helper value ≡ value\n" "BufferRecord′ : (A : Set) → Set \nBufferRecord′ = λ A → let helper : A → A ; helper x = x in Σ value ∶ A • Σ law ∶ helper value ≡ value • ⊤" "-- preceding Agda code\nBufferRecord′ : (A : Set) → Set \nBufferRecord′ = λ A → let helper : A → A ; helper x = x in Σ value ∶ A • Σ law ∶ helper value ≡ value • ⊤\n-- following Agda code\n" 4)"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        agda_editor_tactics_documented_record_parses_and_renders_end_to_end(),
        agda_editor_tactics_realistic_monoid_record_preserves_computed_locals(),
        agda_editor_tactics_dependent_family_workflow_keeps_binder_dependencies(),
        agda_editor_tactics_custom_naming_applies_to_declaration_and_definition(),
        agda_editor_tactics_transforms_a_batch_of_distinct_record_shapes(),
        agda_editor_tactics_editor_buffer_workflow_replaces_a_selected_record(),
    ]
}
