use expect_test::expect;

use super::ParityBatchCase;

fn aidermacs_model_identity_and_price_matching_cover_provider_version_fallbacks() -> ParityBatchCase
{
    ParityBatchCase::value(
        "aidermacs_model_identity_and_price_matching_cover_provider_version_fallbacks",
        r##"(let ((prices
                           '(("openai/gpt-4o" .
                              ((input-price . 0.000005)
                               (output-price . 0.000015)))
                             ("claude-3-5-sonnet" .
                              ((input-price . 0.000003)
                               (output-price . 0.000015)))
                             ("gemini/gemini-2.5-pro-latest" .
                              ((input-price . 0.000001)
                               (output-price . 0.000004))))))
                      (list
                       (mapcar #'aidermacs--parse-model-identity
                               '("openai/gpt-4o-2024-08-06"
                                 "claude-3-5-sonnet-20241022"
                                 "gemini/gemini-2.5-pro-latest"
                                 local-model))
                       (mapcar
                        (lambda (id)
                          (aidermacs--match-model-price id prices))
                        '("openai/gpt-4o-2024-08-06"
                          "claude-3-5-sonnet-20241022"
                          "gemini/gemini-2.5-pro-latest"
                          "missing/model"))))"##,
        expect![[
            r#"OK ((((provider . "openai") (family . "gpt-4o-") (variant . "2024-08-06") (full-id . "openai/gpt-4o-2024-08-06")) ((provider) (family . "claude-3-5-sonnet-20241022") (variant) (full-id . "claude-3-5-sonnet-20241022")) ((provider . "gemini") (family . "gemini-2.5-pro-latest") (variant) (full-id . "gemini/gemini-2.5-pro-latest")) ((provider) (family . "local-model") (variant) (full-id . "local-model"))) (nil nil ((input-price . 1e-06) (output-price . 4e-06)) nil))"#
        ]],
    )
}

fn aidermacs_litellm_json_reader_filters_metadata_and_caches_real_prices() -> ParityBatchCase {
    ParityBatchCase::value(
        "aidermacs_litellm_json_reader_filters_metadata_and_caches_real_prices",
        r##"(let* ((sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
                          (prices-file
                           (expand-file-name "model-prices.json" sandbox))
                          (aidermacs-litellm-prices-file prices-file)
                          (aidermacs--litellm-file-path-cache nil)
                          (aidermacs--litellm-prices-cache nil)
                          (aidermacs--litellm-prices-cache-timestamp nil)
                          (aidermacs-litellm-prices-cache-duration 3600))
                      (with-temp-file prices-file
                        (insert
                         "{"
                         "\"sample_spec\":{\"input_cost_per_token\":99},"
                         "\"openai/gpt-4o\":{\"input_cost_per_token\":0.000005,"
                         "\"output_cost_per_token\":0.000015},"
                         "\"free/model\":{\"input_cost_per_token\":0,"
                         "\"output_cost_per_token\":0},"
                         "\"no-price\":{\"max_tokens\":123}"
                         "}"))
                      (let ((first (aidermacs--get-litellm-prices)))
                        (delete-file prices-file)
                        (list
                         (file-name-nondirectory
                          aidermacs--litellm-file-path-cache)
                         first
                         (equal first (aidermacs--get-litellm-prices))
                         (numberp
                          aidermacs--litellm-prices-cache-timestamp))))"##,
        expect![[
            r#"OK ("model-prices.json" (("openai/gpt-4o" (input-price . 5e-06) (output-price . 1.5e-05)) ("free/model" (input-price . 0) (output-price . 0))) t t)"#
        ]],
    )
}

fn aidermacs_model_ranking_and_annotations_handle_ties_missing_prices_and_limits() -> ParityBatchCase
{
    ParityBatchCase::value(
        "aidermacs_model_ranking_and_annotations_handle_ties_missing_prices_and_limits",
        r##"(let* ((models
                           '(((id . "expensive")
                              (price-str . "($10.00/$20.00/M)"))
                             ((id . "cheap")
                              (price-str . "($0.10/$0.20/M)"))
                             ((id . "unknown") (price-str . ""))
                             ((id . "medium")
                              (price-str . "($2.00/$3.00/M)"))))
                          (ranked
                           (aidermacs--get-cheapest-models models 3))
                          (annotate
                           (aidermacs--make-model-annotator ranked)))
                      (list
                       (mapcar
                        (lambda (model)
                          (list (alist-get 'id model)
                                (aidermacs--model-total-price model)))
                        models)
                       (mapcar
                        (lambda (entry)
                          (list
                           (alist-get 'id (car entry))
                           (cdr entry)))
                        ranked)
                       (mapcar annotate
                               '("cheap" "medium" "expensive" "unknown"))
                       (aidermacs--get-cheapest-models models 0)))"##,
        expect![[
            r#"OK ((("expensive" 30.0) ("cheap" 0.30000000000000004) ("unknown" 999999) ("medium" 5.0)) (("cheap" 1) ("medium" 2) ("expensive" 3)) (" [Rank 1 - Cheapest]" " [Rank 2 - Cheapest]" " [Rank 3 - Cheapest]" nil) nil)"#
        ]],
    )
}

fn aidermacs_available_model_workflow_parses_cli_output_prices_and_invokes_callback()
-> ParityBatchCase {
    ParityBatchCase::value(
        "aidermacs_available_model_workflow_parses_cli_output_prices_and_invokes_callback",
        r##"(let ((aidermacs--cached-models nil)
                          (aidermacs--current-output "")
                          callback-state
                          sent)
                      (cl-letf
                          (((symbol-function 'aidermacs--send-command)
                            (lambda (command _switch _existing redirect callback)
                              (setq sent (list command redirect))
                              (setq aidermacs--current-output
                                    (concat
                                     "Available models:\n"
                                     "- openai/gpt-4o-2024-08-06\n"
                                     "not a model\n"
                                     "- local/free\n"))
                              (funcall callback)))
                           ((symbol-function 'aidermacs--get-litellm-prices)
                            (lambda ()
                              '(("openai/gpt-4o" .
                                 ((input-price . 0.000005)
                                  (output-price . 0.000015)))))))
                        (aidermacs--get-available-models
                         (lambda ()
                           (setq callback-state
                                 (copy-tree aidermacs--cached-models))))
                        (list sent callback-state)))"##,
        expect![[
            r#"OK (("/models /" t) (((id . "openai/gpt-4o-2024-08-06") (price-str . "")) ((id . "local/free") (price-str . ""))))"#
        ]],
    )
}

fn aidermacs_model_selection_updates_main_weak_and_architect_editor_sessions() -> ParityBatchCase {
    ParityBatchCase::value(
        "aidermacs_model_selection_updates_main_weak_and_architect_editor_sessions",
        r##"(let ((aidermacs--cached-models
                           '(((id . "cheap/model")
                              (price-str . "($0.10/$0.20/M)"))
                             ((id . "smart/model")
                              (price-str . "($2.00/$4.00/M)"))))
                          (aidermacs-default-model "old")
                          (aidermacs-weak-model nil)
                          (aidermacs-architect-model nil)
                          (aidermacs-editor-model nil)
                          sent
                          reads)
                      (cl-letf
                          (((symbol-function 'aidermacs-aider-version)
                            (lambda () "0.80.0"))
                           ((symbol-function 'aidermacs--send-command)
                            (lambda (command &rest _)
                              (push command sent)))
                           ((symbol-function 'completing-read)
                            (lambda (prompt collection &rest _)
                              (push prompt reads)
                              (cond
                               ((string-prefix-p "Select model type" prompt)
                                "Editing Model")
                               (t
                                (let ((choices
                                       (all-completions "" collection)))
                                  (cl-find-if
                                   (lambda (choice)
                                     (string-prefix-p "smart/model" choice))
                                   choices)))))))
                        (let ((aidermacs--current-mode 'code))
                          (aidermacs--select-model nil))
                        (let ((aidermacs--current-mode 'code))
                          (aidermacs--select-model t))
                        (let ((aidermacs--current-mode 'architect))
                          (aidermacs--select-model nil))
                        (list
                         aidermacs-default-model
                         aidermacs-weak-model
                         aidermacs-architect-model
                         aidermacs-editor-model
                         (nreverse sent)
                         (nreverse reads))))"##,
        expect![[
            r#"OK ("smart/model" "smart/model" nil "smart/model" ("/model smart/model" "/weak-model smart/model" "/editor-model smart/model") ("Select Main Model: " "Select Weak Model: " "Select model type: " "Select Editing Model: "))"#
        ]],
    )
}

pub(super) fn models_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        aidermacs_model_identity_and_price_matching_cover_provider_version_fallbacks(),
        aidermacs_litellm_json_reader_filters_metadata_and_caches_real_prices(),
        aidermacs_model_ranking_and_annotations_handle_ties_missing_prices_and_limits(),
        aidermacs_available_model_workflow_parses_cli_output_prices_and_invokes_callback(),
        aidermacs_model_selection_updates_main_weak_and_architect_editor_sessions(),
    ]
}
