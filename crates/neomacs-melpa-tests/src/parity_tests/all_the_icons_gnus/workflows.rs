use expect_test::expect;

use super::ParityBatchCase;

/// The package is installed by calling one function, and all it does is rewrite
/// Gnus's line formats.  Pinned here: the stock format of every variable it
/// touches, the format afterwards with each icon glyph and the font it comes
/// from, and that calling the setup a second time leaves everything exactly as
/// the first call did.
fn the_setup_function_rewrites_every_gnus_line_format_it_owns() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_setup_function_rewrites_every_gnus_line_format_it_owns",
        r##"(let ((before (aig-test-formats)))
  (all-the-icons-gnus-setup)
  (let ((after (aig-test-formats)))
    (all-the-icons-gnus-setup)
    (list :before before
          :after after
          :idempotent (equal after (aig-test-formats)))))"##,
        expect![[
            r#"OK (:before (:summary ("%U%R%z%I%(%[%4L: %-23,23f%]%) %s\n" nil) :group ("%M%S%p%P%5y:%B%(%g%)\n" nil) :topic ("%i[ %(%{%n%}%) -- %A ]%v\n" nil) :user-date ("%b %d %Y" nil) :tree-root ("> " nil) :tree-false-root ("> " nil) :tree-vertical ("| " nil) :tree-single-leaf ("\\-> " nil)) :after (:summary ("%1{%U%R%z: %}%[%2{%&user-date;%}%] <icon 59389> %4{%-34,34n%} %3{<icon 57699> %}%(%1{%B%}%s%)\n" ((35 59389 "Material Icons") (54 57699 "Material Icons"))) :group ("%1M%1S%5y <icon 57688> : %(%-50,50G%)\n" ((10 57688 "Material Icons"))) :topic ("%i[ <icon 58055> %(%{%n -- %A%}%) ]%v\n" ((4 58055 "Material Icons"))) :user-date ("<icon 59670> %Y-%m-%d %H:%M" ((0 59670 "Material Icons"))) :tree-root ("<icon 57688> " ((0 57688 "Material Icons"))) :tree-false-root ("<icon 57684> " ((0 57684 "Material Icons"))) :tree-vertical (" " nil) :tree-single-leaf ("<icon 57688> " ((0 57688 "Material Icons")))) :idempotent t)"#
        ]],
    )
}

fn a_real_mbox_group_renders_its_summary_through_the_icon_formats() -> ParityBatchCase {
    ParityBatchCase::value(
        "a_real_mbox_group_renders_its_summary_through_the_icon_formats",
        r##"(let ((path (aig-test-write "mail/inbox.mbox" aig-test-mbox)))
  (aig-test-prepare-gnus)
  (unwind-protect
      (progn
        (aig-test-open-mbox "stock" path)
        (let ((stock (aig-test-summary-render)))
          (aig-test-kill-gnus-buffers)
          (all-the-icons-gnus-setup)
          (aig-test-open-mbox "iconised" path)
          (let ((iconised (aig-test-summary-render)))
            (list :stock stock
                  :iconised iconised
                  :buffer-name (and (aig-test-summary-buffer)
                                    (buffer-name (aig-test-summary-buffer)))))))
    (aig-test-kill-gnus-buffers)))"##,
        expect![[
            r#"OK (:stock (:mode gnus-summary-mode :lines ((" . [   2: Alice Adams            ] Release plan" nil) (" .     [   2: Bob Brown              ] " nil))) :iconised (:mode gnus-summary-mode :lines ((" . : [<icon 59670> 2024-01-01 10:00] <icon 59389> Alice Adams                        <icon 57699> <icon 57688> Release plan" ((6 59670 nil) (26 59389 nil) (63 57699 nil) (65 57688 nil))) (" . : [<icon 59670> 2024-01-02 11:30] <icon 59389> Bob Brown                          <icon 57699> <icon 57688> " ((6 59670 nil) (26 59389 nil) (63 57699 nil) (65 57688 nil))))) :buffer-name "*Summary nndoc+[ORACLE-SANDBOX]/mail/inbox.mbox-ephemeral:iconised*")"#
        ]],
    )
    .fresh_process()
}

fn the_header_composition_command_needs_dash_and_its_own_label_shape() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_header_composition_command_needs_dash_and_its_own_label_shape",
        r##"(list
 :without-dash
 (list :dash-loaded (featurep 'dash)
       :outcome (with-temp-buffer
                  (insert "From:  : Alice\n")
                  (condition-case failure
                      (progn (all-the-icons-gnus--add-faces) 'composed)
                    (error failure))))
 :real-article
 (progn
   (require 'dash)
   (with-temp-buffer
     (insert "From: Alice Adams <alice@example.org>\n"
             "Subject: Release plan\n"
             "To: team@example.org\n"
             "Date: Mon, 1 Jan 2024 10:00:00 +0000\n"
             "\n"
             "Let us ship on Friday.\n")
     (all-the-icons-gnus--add-faces)
     (list :compositions (aig-test-compositions)
           :text (buffer-substring-no-properties (point-min) (point-max)))))
 :labels-as-the-group-format-writes-them
 (with-temp-buffer
   (insert "From:  : Alice Adams\n"
           "Subject:  : Release plan\n"
           "CC:  : nobody\n"
           "Body text.\n")
   (all-the-icons-gnus--add-faces)
   (list :compositions (aig-test-compositions)
         :text (buffer-substring-no-properties (point-min) (point-max)))))"##,
        expect![[
            r##"OK (:without-dash (:dash-loaded nil :outcome (void-function --each)) :real-article (:compositions nil :text "From: Alice Adams <alice@example.org>\nSubject: Release plan\nTo: team@example.org\nDate: Mon, 1 Jan 2024 10:00:00 +0000\n\nLet us ship on Friday.\n") :labels-as-the-group-format-writes-them (:compositions (("From:  : " (:foreground "#375E97")) ("Subject:  : " (:foreground "#375E97")) ("CC:  : " (:foreground "#375E97"))) :text "From:  : Alice Adams\nSubject:  : Release plan\nCC:  : nobody\nBody text.\n"))"##
        ]],
    )
}

fn the_package_builds_a_table_of_header_labels_and_their_icons() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_package_builds_a_table_of_header_labels_and_their_icons",
        r##"(list
 :entries (length pretty-gnus-article-alist)
 :table (mapcar (lambda (entry)
                  (list (nth 0 entry)
                        (aig-test-render (nth 1 entry))
                        (nth 2 entry)))
                pretty-gnus-article-alist)
 :faces-are-plists (cl-every (lambda (entry) (plist-get (nth 2 entry) :foreground))
                             pretty-gnus-article-alist))"##,
        expect![[
            r##"OK (:entries 11 :table (("\\<\\(X-PGP-Fingerprint:  : \\)" ("<icon 62014>" ((0 62014 "FontAwesome"))) (:foreground "#375E97")) ("\\<\\(X-mailer:  : \\)" ("<icon 62056>" ((0 62056 "FontAwesome"))) (:foreground "#375E97")) ("\\<\\(User-Agent:  : \\)" ("<icon 62056>" ((0 62056 "FontAwesome"))) (:foreground "#375E97")) ("\\<\\(Content-Type:  : \\)" ("<icon 61529>" ((0 61529 "FontAwesome"))) (:foreground "#375E97")) ("\\<\\(Organization:  : \\)" ("<icon 61852>" ((0 61852 "FontAwesome"))) (:foreground "#375E97")) ("\\<\\(Date:  : \\)" ("<icon 61555>" ((0 61555 "FontAwesome"))) (:foreground "#375E97")) ("\\<\\(Reply-To:  : \\)" ("<icon 61579>" ((0 61579 "FontAwesome"))) (:foreground "#375E97")) ("\\<\\(CC:  : \\)" ("<icon 61632>" ((0 61632 "github-octicons"))) (:foreground "#375E97")) ("\\<\\(To:  : \\)" ("<icon 61447>" ((0 61447 "FontAwesome"))) (:foreground "#375E97")) ("\\<\\(Subject:  : \\)" ("<icon 61443>" ((0 61443 "FontAwesome"))) (:foreground "#375E97")) ("\\<\\(From:  : \\)" ("<icon 61447>" ((0 61447 "FontAwesome"))) (:foreground "#375E97"))) :faces-are-plists t)"##
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        the_setup_function_rewrites_every_gnus_line_format_it_owns(),
        a_real_mbox_group_renders_its_summary_through_the_icon_formats(),
        the_header_composition_command_needs_dash_and_its_own_label_shape(),
        the_package_builds_a_table_of_header_labels_and_their_icons(),
    ]
}
