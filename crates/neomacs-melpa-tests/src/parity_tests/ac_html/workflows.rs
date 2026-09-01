use expect_test::expect;

use super::ParityBatchCase;

/// The package's primary story: type `<di', pick a tag, keep writing.  The
/// candidates come from the shipped `completion-data/html-tag-list' and carry
/// the tag source's "t" symbol.
fn completing_html_tags_while_writing_a_real_document() -> ParityBatchCase {
    ParityBatchCase::value(
        "completing_html_tags_while_writing_a_real_document",
        r####"
(aht-test-in-buffer
 (insert "<di")
 (let* ((candidates (aht-test-candidates))
        (prefix ac-prefix)
        (symbols (delete-dups
                  (mapcar (lambda (i) (get-text-property 0 'symbol i)) ac-candidates))))
   (ac-complete)
   (let ((first (list :buffer (buffer-string) :point (point))))
     (insert " ")
     (goto-char (point-max))
     (insert ">alpha</div>\n<inp")
     (let ((second-candidates (aht-test-candidates)))
       (ac-complete)
       (list :mode major-mode
             :providers ac-html-enabled-data-providers
             :sources ac-sources
             :prefix prefix
             :candidates candidates
             :symbols symbols
             :after-first first
             :second-candidates second-candidates
             :buffer (buffer-string)
             :point (point)
             :modified (buffer-modified-p))))))
"####,
        expect![[
            r#"OK (:mode html-mode :providers (ac-html-default-data-provider) :sources (ac-source-html-tag ac-source-html-attr ac-source-html-attrv) :prefix "di" :candidates ("dir" "div" "dialog") :symbols ("t") :after-first (:buffer "<dir" :point 5) :second-candidates ("input") :buffer "<dir >alpha</div>\n<input" :point 25 :modified t)"#
        ]],
    )
}

fn the_tag_sources_prefix_shadows_attribute_completion() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_tag_sources_prefix_shadows_attribute_completion",
        r####"
(aht-test-in-buffer
 (list
  :with-tag-source-first (aht-test-offer "<div cl")
  :with-attr-source-only
  (let ((ac-sources '(ac-source-html-attr)))
    (list (aht-test-offer "<div cl")
          (aht-test-offer "<a hr")
          (aht-test-offer "<div id=\"main\" ta")))
  :tag-prefix-claims-the-whole-run
  (progn (erase-buffer)
         (insert "<div cl")
         (list :tag-prefix (save-excursion (ac-html-tag-prefix))
               :attr-prefix (save-excursion (ac-html-attr-prefix))
               :current-tag (save-excursion (ac-html-current-tag))))))
"####,
        expect![[
            r#"OK (:with-tag-source-first (:typed "<div cl" :prefix "div cl" :count 0 :symbols nil :candidates nil) :with-attr-source-only ((:typed "<div cl" :prefix "cl" :count 1 :symbols ("a") :candidates ("class")) (:typed "<a hr" :prefix "hr" :count 2 :symbols ("a") :candidates ("href" "hreflang")) (:typed "<div id=\"main\" ta" :prefix "ta" :count 1 :symbols ("a") :candidates ("tabindex"))) :tag-prefix-claims-the-whole-run (:tag-prefix 2 :attr-prefix 6 :current-tag "div"))"#
        ]],
    )
}

fn completing_an_attribute_value_inside_a_quoted_string() -> ParityBatchCase {
    ParityBatchCase::value(
        "completing_an_attribute_value_inside_a_quoted_string",
        r####"
(aht-test-in-buffer
 (list
  :input-type (aht-test-offer-with-docs "<input type=\"che")
  :a-target (let ((ac-sources '(ac-source-html-attrv)))
              (aht-test-offer "<a target=\"_"))
  :current-attr (progn (erase-buffer)
                       (insert "<input type=\"che")
                       (list :tag (save-excursion (ac-html-current-tag))
                             :attr (save-excursion (ac-html-current-attr))))
  :complete
  (progn (erase-buffer)
         (insert "<input type=\"che")
         (aht-test-candidates)
         (ac-complete)
         (list :buffer (buffer-string) :point (point)))))
"####,
        expect![[
            r#"OK (:input-type (:typed "<input type=\"che" :prefix "che" :candidates ("checkbox") :documentation (("checkbox" "v" "A set of zero or more values from a predefined list.nA checkboxn\n"))) :a-target (:typed "<a target=\"_" :prefix "_" :count 4 :symbols ("v") :candidates ("_top" "_self" "_blank" "_parent")) :current-attr (:tag "input" :attr "type") :complete (:buffer "<input type=\"checkbox" :point 22))"#
        ]],
    )
}

fn candidate_documentation_is_read_from_the_shipped_data_files() -> ParityBatchCase {
    ParityBatchCase::value(
        "candidate_documentation_is_read_from_the_shipped_data_files",
        r####"
(aht-test-in-buffer
 (list
  :tag-docs (aht-test-offer-with-docs "<inp")
  :attrv-docs (let ((ac-sources '(ac-source-html-attrv)))
                (aht-test-offer-with-docs "<input type=\"rad"))
  :direct (list :tag (ac-html-tag-documentation "div")
                :attr (progn (erase-buffer) (insert "<img sr")
                             (ac-html-attr-documentation "src"))
                :missing-tag (ac-html-tag-documentation "notatag"))))
"####,
        expect![[
            r#"OK (:tag-docs (:typed "<inp" :prefix "inp" :candidates ("input") :documentation (("input" "t" "The HTML <input> element is used to create interactive controls for web-based forms in order to accept data from the user. The semantics of an <input> varies considerably depending on the value of its type attribute.\n\nContent categories:\nFlow content, listed, submittable, resettable, form-associated element, phrasing content.\nIf the type has not the hidden value, labellable element, palpable content.\n\nPermitted content:\nNone, it is an empty element.\n\nTag omission:\nMust have a start tag and must not have an end tag.\n\nPermitted parent elements:\nAny element that accepts phrasing content.\n\nDOM interface:\nHTMLInputElement"))) :attrv-docs (:typed "<input type=\"rad" :prefix "rad" :candidates ("radio") :documentation (("radio" "v" "An enumerated value.nA radio buttonn\n"))) :direct (:tag "The HTML <div> element (or HTML Document Division Element) is the generic container for flow content, which does not inherently represent anything. It can be used to group elements for styling purposes (using the class or id attributes), or because they share attribute values, such as lang. It should be used only when no other semantic element (such as <article> or <nav>) is appropriate.\n\nContent categories:\nFlow content, palpable content.\n\nPermitted content:\nFlow content.\n\nTag omission:\nNone, both the starting and ending tag are mandatory.\n\nPermitted parent elements:\nAny element that accepts flow content.\n\nDOM interface:\nHTMLDivElement" :attr "src\n\nImage URL, this attribute is obligatory for the <img> element. On browsers supporting srcset, src is ignored if this one is provided." :missing-tag nil))"#
        ]],
    )
}

fn enabling_a_second_data_provider_adds_its_tags_to_the_offer() -> ParityBatchCase {
    ParityBatchCase::value(
        "enabling_a_second_data_provider_adds_its_tags_to_the_offer",
        r####"
(aht-test-in-buffer
 (let ((before (list :providers ac-html-enabled-data-providers
                     :offer (aht-test-offer "<tag"))))
   (require 'ac-html-testing-data-provider)
   (ac-html-enable-data-provider 'ac-html-testing-data-provider)
   (let ((after (list :providers ac-html-enabled-data-providers
                      :offer (aht-test-offer "<tag")
                      :still-has-defaults (aht-test-offer "<inp"))))
     (list :registered ac-html-data-providers
           :before before
           :after after
           :provider-keys
           (list (ac-html-query-data-provider 'ac-html-testing-data-provider :tag-func)
                 (ac-html-query-data-provider 'ac-html-testing-data-provider :class-func)
                 (ac-html-query-data-provider 'ac-html-default-data-provider :tag-doc-func))))))
"####,
        expect![[
            r#"OK (:registered (ac-html-testing-data-provider ac-html-default-data-provider) :before (:providers #1=(ac-html-default-data-provider) :offer (:typed "<tag" :prefix "tag" :count 0 :symbols nil :candidates nil)) :after (:providers (ac-html-testing-data-provider . #1#) :offer (:typed "<tag" :prefix "tag" :count 3 :symbols ("t") :candidates ("tag1" "tag2" "tag3")) :still-has-defaults (:typed "<inp" :prefix "inp" :count 1 :symbols ("t") :candidates ("input"))) :provider-keys (ac-html-testing-tags ac-html-testing-classes ac-html-default-tag-doc))"#
        ]],
    )
}

fn class_and_id_values_come_from_the_providers_class_and_id_functions() -> ParityBatchCase {
    ParityBatchCase::value(
        "class_and_id_values_come_from_the_providers_class_and_id_functions",
        r####"
(aht-test-in-buffer
 (require 'ac-html-testing-data-provider)
 (ac-html-enable-data-provider 'ac-html-testing-data-provider)
 (let ((ac-sources '(ac-source-html-attrv)))
   (list :class (aht-test-offer "<div class=\"cl")
         :id (aht-test-offer "<div id=\"id")
         :other-attr (aht-test-offer "<input type=\"che")
         :class-candidates (ac-html-all-class-candidates)
         :id-candidates (ac-html-all-id-candidates))))
"####,
        expect![[
            r#"OK (:class (:typed "<div class=\"cl" :prefix "cl" :count 2 :symbols ("v") :candidates ("class1" "class2")) :id (:typed "<div id=\"id" :prefix "id" :count 3 :symbols ("v") :candidates ("id1" "id2" "id3")) :other-attr (:typed "<input type=\"che" :prefix "che" :count 1 :symbols ("v") :candidates ("checkbox")) :class-candidates ("class1" "class2") :id-candidates ("id1" "id2" "id3"))"#
        ]],
    )
}

fn an_unknown_tag_offers_nothing_and_leaves_the_document_untouched() -> ParityBatchCase {
    ParityBatchCase::value(
        "an_unknown_tag_offers_nothing_and_leaves_the_document_untouched",
        r####"
(aht-test-in-buffer
 (insert "<p>keep me</p>\n<zzq")
 (let ((before (buffer-string))
       (point-before (point)))
   (let ((candidates (aht-test-candidates))
         (prefix ac-prefix))
     (ac-complete)
     (let ((after (buffer-string)))
       (goto-char (point-max))
       (insert "\n<inp")
       (let ((recovered (aht-test-candidates)))
         (ac-complete)
         (list :before before :point-before point-before
               :prefix prefix :candidates candidates
               :after after :point-after (point)
               :unchanged (equal before after)
               :recovered recovered
               :final (buffer-string)))))))
"####,
        expect![[
            r#"OK (:before "<p>keep me</p>\n<zzq" :point-before 20 :prefix "zzq" :candidates nil :after "<p>keep me</p>\n<zzq" :point-after 27 :unchanged t :recovered ("input") :final "<p>keep me</p>\n<zzq\n<input")"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        completing_html_tags_while_writing_a_real_document(),
        the_tag_sources_prefix_shadows_attribute_completion(),
        completing_an_attribute_value_inside_a_quoted_string(),
        candidate_documentation_is_read_from_the_shipped_data_files(),
        enabling_a_second_data_provider_adds_its_tags_to_the_offer(),
        class_and_id_values_come_from_the_providers_class_and_id_functions(),
        an_unknown_tag_offers_nothing_and_leaves_the_document_untouched(),
    ]
}
