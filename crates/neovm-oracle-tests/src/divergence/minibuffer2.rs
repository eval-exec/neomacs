//! Minibuffer read-function prompt/print divergence probes.
//!
//! Confirmed bug (batch 1): Neomacs prints the minibuffer prompt
//! inconsistently across the various `read-*` functions on EOF. This file
//! probes each safe completing/symbol/string reader individually (only those
//! that error end-of-file on closed stdin; char/event/key/passwd readers are
//! excluded to avoid hangs). Each test surfaces whether Neomacs prints the
//! prompt + returns the same EOF value as GNU.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

macro_rules! read_eof {
    ($name:ident, $form:expr) => {
        #[test]
        fn $name() {
            return_if_neovm_enable_oracle_proptest_not_set!();
            crate::common::assert_oracle_parity($form);
        }
    };
}

read_eof!(
    div_mb2_read_buffer_prompt,
    r##"(condition-case e (read-buffer "BP: " "x" t) (error (car e)))"##
);
read_eof!(
    div_mb2_read_variable_prompt,
    r##"(condition-case e (read-variable "VP: " 'x) (error (car e)))"##
);
read_eof!(
    div_mb2_read_regexp_prompt,
    r##"(condition-case e (read-regexp "RP: ") (error (car e)))"##
);
read_eof!(
    div_mb2_read_command_prompt,
    r##"(condition-case e (read-command "CP: ") (error (car e)))"##
);
read_eof!(
    div_mb2_read_function_prompt,
    r##"(condition-case e (read-function "FP: ") (error (car e)))"##
);
read_eof!(
    div_mb2_read_face_prompt,
    r##"(condition-case e (read-face "FaP: ") (error (car e)))"##
);
read_eof!(
    div_mb2_read_color_prompt,
    r##"(condition-case e (read-color "ColP: ") (error (car e)))"##
);
read_eof!(
    div_mb2_read_coding_system_prompt,
    r##"(condition-case e (read-coding-system "CsP: ") (error (car e)))"##
);
read_eof!(
    div_mb2_read_input_method_prompt,
    r##"(condition-case e (read-input-method "ImP: ") (error (car e)))"##
);
read_eof!(
    div_mb2_read_env_var_prompt,
    r##"(condition-case e (read-environment-variable-name "EvP: ") (error (car e)))"##
);
read_eof!(
    div_mb2_read_library_prompt,
    r##"(condition-case e (read-library "LibP: ") (error (car e)))"##
);
read_eof!(
    div_mb2_read_file_name_prompt,
    r##"(condition-case e (read-file-name "FNP: ") (error (car e)))"##
);
read_eof!(
    div_mb2_read_directory_name_prompt,
    r##"(condition-case e (read-directory-name "DNP: ") (error (car e)))"##
);
read_eof!(
    div_mb2_read_shell_command_prompt,
    r##"(condition-case e (read-shell-command "ShP: ") (error (car e)))"##
);
read_eof!(
    div_mb2_read_no_blanks_prompt,
    r##"(condition-case e (read-no-blanks "NbP: ") (error (car e)))"##
);
read_eof!(
    div_mb2_read_char_choice_eof,
    r##"(condition-case e (read-char-choice "ChCP: " '(?y ?n)) (error (car e)))"##
);
read_eof!(
    div_mb2_read_multiple_choice_eof,
    r##"(condition-case e (read-multiple-choice "MCP: " '((?a "opt a"))) (error (car e)))"##
);
read_eof!(
    div_mb2_read_one_window_prompt,
    r##"(condition-case e (read-window "WinP: " nil t) (error (car e)))"##
);
read_eof!(
    div_mb2_read_activated_input_method,
    r##"(condition-case e (read-activated-input-method-name "AiP: ") (error (car e)))"##
);
read_eof!(
    div_mb2_read_minor_mode_prompt,
    r##"(condition-case e (read-minor-mode "MmP: ") (error (car e)))"##
);
