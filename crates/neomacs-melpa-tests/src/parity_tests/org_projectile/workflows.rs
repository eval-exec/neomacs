use expect_test::expect;

use super::ParityBatchCase;

fn current_project_capture_writes_a_real_per_project_todo_file() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-org-projectile-test-with-sandbox "per-project-capture"
  (let* ((project-root
          (neomacs-org-projectile-test-project "services/api service"))
         (default-directory project-root)
         (projectile-known-projects (list project-root))
         (todo-file (expand-file-name "TODO.org" project-root)))
    (org-projectile-per-project)
    (org-projectile-capture-for-current-project
     :capture-template
     "* NEXT Deploy canary\n:PROPERTIES:\n:OWNER: Zoë\n:END:\n"
     :immediate-finish t)
    (let ((todo-buffer (get-file-buffer todo-file)))
      (list
       :project
       (list :root (file-relative-name (projectile-project-root) case-root)
             :name (projectile-project-name)
             :strategy (eieio-object-class-name org-projectile-strategy))
       :todo
       (list :exists (file-exists-p todo-file)
             :relative-path (file-relative-name todo-file case-root)
             :text (neomacs-org-projectile-test-file-text todo-file)
             :mode (and todo-buffer
                        (buffer-local-value 'major-mode todo-buffer))
             :headings
             (and todo-buffer
                  (with-current-buffer todo-buffer
                    (org-map-entries
                     #'neomacs-org-projectile-test-heading-record))))
       :discovery
       (mapcar (lambda (path) (file-relative-name path case-root))
               (org-projectile-todo-files))
       :capture-buffers (neomacs-org-projectile-test-capture-buffers)
       :origin-restored
       (eq (window-buffer (selected-window)) origin-buffer)))))
"####;
    let expected = expect![[
        r#"OK (:project (:root "services/api service/" :name "api service" :strategy org-projectile-per-project-strategy) :todo (:exists t :relative-path "services/api service/TODO.org" :text ":PROPERTIES:\n:CATEGORY: api service\n:END:\n* NEXT Deploy canary\n:PROPERTIES:\n:OWNER: Zoë\n:END:\n" :mode org-mode :headings ((:level 1 :todo "NEXT" :heading "Deploy canary" :stats nil :category "api service" :owner "Zoë"))) :discovery ("services/api service/TODO.org") :capture-buffers nil :origin-restored t)"#
    ]];
    ParityBatchCase::value(
        "current_project_capture_writes_a_real_per_project_todo_file",
        elisp_form,
        expected,
    )
}

fn completion_and_navigation_keep_same_basename_projects_distinct() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-org-projectile-test-with-sandbox "single-file-navigation"
  (let* ((alpha-root
          (neomacs-org-projectile-test-project "departments/alpha/service"))
         (beta-root
          (neomacs-org-projectile-test-project "departments/beta/service"))
         (projectile-known-projects (list alpha-root beta-root))
         (projectile-project-name-function
          (lambda (project-root)
            (let* ((service
                    (file-name-nondirectory
                     (directory-file-name project-root)))
                   (parent
                    (file-name-nondirectory
                     (directory-file-name
                      (file-name-directory
                       (directory-file-name project-root))))))
              (format "%s/%s" parent service))))
         (portfolio (expand-file-name "portfolio.org" case-root))
         (org-project-capture-projects-file portfolio)
         (default-directory alpha-root)
         (unread-command-events nil)
         selected beta-view alpha-view)
    (neomacs-org-projectile-test-write portfolio "#+title: Project portfolio\n\n")
    (org-projectile-single-file)
    (let ((executing-kbd-macro t)
          (unread-command-events
           (listify-key-sequence (kbd "beta/service RET"))))
      (setq selected
            (org-projectile-completing-read
             "Open project TODO: " nil t)))
    (org-projectile-goto-location-for-project selected)
    (setq beta-view
          (list :buffer (file-relative-name (buffer-file-name) case-root)
                :point-heading (neomacs-org-projectile-test-heading-record)
                :modified (buffer-modified-p)))
    (save-buffer)
    (org-projectile-goto-location-for-project "alpha/service")
    (setq alpha-view
          (list :buffer (file-relative-name (buffer-file-name) case-root)
                :point-heading (neomacs-org-projectile-test-heading-record)
                :modified (buffer-modified-p)))
    (save-buffer)
    (list
     :selected selected
     :project-names
     (mapcar #'projectile-project-name (list alpha-root beta-root))
     :strategy (eieio-object-class-name org-projectile-strategy)
     :beta-view beta-view
     :alpha-view alpha-view
     :portfolio
     (list :text (neomacs-org-projectile-test-file-text portfolio)
           :headings
           (org-map-entries #'neomacs-org-projectile-test-heading-record))
     :discovery
     (mapcar (lambda (path) (file-relative-name path case-root))
             (org-projectile-todo-files))
     :capture-buffers (neomacs-org-projectile-test-capture-buffers))))
"####;
    let expected = expect![[
        r##"OK (:selected "beta/service" :project-names ("alpha/service" "beta/service") :strategy org-projectile-single-file-strategy :beta-view (:buffer "portfolio.org" :point-heading (:level 1 :todo nil :heading "[[elisp:(org-project-capture-open-project \"beta/service\")][beta/service]] [/]" :stats nil :category "beta/service" :owner nil) :modified t) :alpha-view (:buffer "portfolio.org" :point-heading (:level 1 :todo nil :heading "[[elisp:(org-project-capture-open-project \"alpha/service\")][alpha/service]] [/]" :stats nil :category "alpha/service" :owner nil) :modified t) :portfolio (:text "#+title: Project portfolio\n* [[elisp:(org-project-capture-open-project \"beta/service\")][beta/service]] [/]\n:PROPERTIES:\n:CATEGORY: beta/service\n:END:\n* [[elisp:(org-project-capture-open-project \"alpha/service\")][alpha/service]] [/]\n:PROPERTIES:\n:CATEGORY: alpha/service\n:END:\n" :headings ((:level 1 :todo nil :heading "[[elisp:(org-project-capture-open-project \"beta/service\")][beta/service]] [/]" :stats 0 :category "beta/service" :owner nil) (:level 1 :todo nil :heading "[[elisp:(org-project-capture-open-project \"alpha/service\")][alpha/service]] [/]" :stats 0 :category "alpha/service" :owner nil))) :discovery ("portfolio.org") :capture-buffers nil)"##
    ]];
    ParityBatchCase::value(
        "completion_and_navigation_keep_same_basename_projects_distinct",
        elisp_form,
        expected,
    )
}

fn choosing_a_project_captures_only_in_that_projects_file() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-org-projectile-test-with-sandbox "choose-project-capture"
  (let* ((web-root (neomacs-org-projectile-test-project "products/web"))
         (api-root (neomacs-org-projectile-test-project "services/api"))
         (projectile-known-projects (list web-root api-root))
         (web-todo (expand-file-name "TODO.org" web-root))
         (api-todo (expand-file-name "TODO.org" api-root))
         (default-directory web-root)
         (unread-command-events nil))
    (org-projectile-per-project)
    (let ((executing-kbd-macro t)
          (unread-command-events (listify-key-sequence (kbd "api RET"))))
      (org-projectile-project-todo-completing-read
       :capture-template
       "* TODO Rotate signing key\n:PROPERTIES:\n:OWNER: SRE Ω\n:END:\n"
       :immediate-finish t))
    (let ((api-buffer (get-file-buffer api-todo)))
      (list
       :web-created (file-exists-p web-todo)
       :api
       (list :created (file-exists-p api-todo)
             :text (neomacs-org-projectile-test-file-text api-todo)
             :headings
             (with-current-buffer api-buffer
               (org-map-entries
                #'neomacs-org-projectile-test-heading-record)))
       :discovery
       (mapcar (lambda (path) (file-relative-name path case-root))
               (org-projectile-todo-files))
       :capture-buffers (neomacs-org-projectile-test-capture-buffers)
       :origin-restored
       (eq (window-buffer (selected-window)) origin-buffer)))))
"####;
    let expected = expect![[
        r#"OK (:web-created nil :api (:created t :text ":PROPERTIES:\n:CATEGORY: api\n:END:\n* TODO Rotate signing key\n:PROPERTIES:\n:OWNER: SRE Ω\n:END:\n" :headings ((:level 1 :todo "TODO" :heading "Rotate signing key" :stats nil :category "api" :owner "SRE Ω"))) :discovery ("services/api/TODO.org") :capture-buffers nil :origin-restored t)"#
    ]];
    ParityBatchCase::value(
        "choosing_a_project_captures_only_in_that_projects_file",
        elisp_form,
        expected,
    )
}

fn org_capture_template_routes_a_source_file_todo_to_its_project() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-org-projectile-test-with-sandbox "org-capture-template"
  (let* ((project-root (neomacs-org-projectile-test-project "libraries/parser"))
         (source-file (expand-file-name "src/parser.el" project-root))
         (todo-file (expand-file-name "TODO.org" project-root))
         (projectile-known-projects (list project-root))
         (default-directory project-root))
    (neomacs-org-projectile-test-write
     source-file "(defun parser-read-token () nil)\n")
    (find-file source-file)
    (setq org-capture-templates
          (list
           (org-projectile-project-todo-entry
            :capture-character "p"
            :capture-heading "Parser maintenance"
            :capture-template
            "* TODO Document token escapes\n:PROPERTIES:\n:SOURCE: parser.el\n:END:\n"
            :immediate-finish t)))
    (org-capture nil "p")
    (let ((todo-buffer (get-file-buffer todo-file)))
      (list
       :source (file-relative-name (buffer-file-name) case-root)
       :source-text (neomacs-org-projectile-test-file-text source-file)
       :todo
       (list :text (neomacs-org-projectile-test-file-text todo-file)
             :headings
             (with-current-buffer todo-buffer
               (org-map-entries
                #'neomacs-org-projectile-test-heading-record)))
       :capture-buffers (neomacs-org-projectile-test-capture-buffers)
       :source-restored (equal (buffer-file-name) source-file)))))
"####;
    let expected = expect![[
        r#"OK (:source "libraries/parser/src/parser.el" :source-text "(defun parser-read-token () nil)\n" :todo (:text ":PROPERTIES:\n:CATEGORY: parser\n:END:\n* TODO Document token escapes\n:PROPERTIES:\n:SOURCE: parser.el\n:END:\n" :headings ((:level 1 :todo "TODO" :heading "Document token escapes" :stats nil :category "parser" :owner nil))) :capture-buffers nil :source-restored t)"#
    ]];
    ParityBatchCase::value(
        "org_capture_template_routes_a_source_file_todo_to_its_project",
        elisp_form,
        expected,
    )
}

fn single_file_capture_creates_a_linked_project_heading_and_child_todo() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-org-projectile-test-with-sandbox "single-file-capture"
  (let* ((project-root
          (neomacs-org-projectile-test-project "products/control plane"))
         (projectile-known-projects (list project-root))
         (portfolio (expand-file-name "portfolio.org" case-root))
         (org-project-capture-projects-file portfolio)
         (default-directory project-root))
    (neomacs-org-projectile-test-write portfolio "#+title: Delivery portfolio\n\n")
    (org-projectile-single-file)
    (org-projectile-capture-for-current-project
     :capture-template
     "* NEXT Harden scheduler\n:PROPERTIES:\n:OWNER: 李\n:END:\n"
     :immediate-finish t)
    (let ((portfolio-buffer (get-file-buffer portfolio)))
      (list
       :strategy (eieio-object-class-name org-projectile-strategy)
       :portfolio
       (list :text (neomacs-org-projectile-test-file-text portfolio)
             :headings
             (with-current-buffer portfolio-buffer
               (org-map-entries
                #'neomacs-org-projectile-test-heading-record)))
       :capture-buffers (neomacs-org-projectile-test-capture-buffers)
       :origin-restored
       (eq (window-buffer (selected-window)) origin-buffer)))))
"####;
    let expected = expect![[
        r##"OK (:strategy org-projectile-single-file-strategy :portfolio (:text "#+title: Delivery portfolio\n* [[elisp:(org-project-capture-open-project \"control plane\")][control plane]] [0/1]\n:PROPERTIES:\n:CATEGORY: control plane\n:END:\n** NEXT Harden scheduler\n:PROPERTIES:\n:OWNER: 李\n:END:\n" :headings ((:level 1 :todo nil :heading "[[elisp:(org-project-capture-open-project \"control plane\")][control plane]] [0/1]" :stats 0 :category "control plane" :owner nil) (:level 2 :todo "NEXT" :heading "Harden scheduler" :stats 0 :category "control plane" :owner "李"))) :capture-buffers nil :origin-restored t)"##
    ]];
    ParityBatchCase::value(
        "single_file_capture_creates_a_linked_project_heading_and_child_todo",
        elisp_form,
        expected,
    )
    .fresh_process()
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        current_project_capture_writes_a_real_per_project_todo_file(),
        completion_and_navigation_keep_same_basename_projects_distinct(),
        choosing_a_project_captures_only_in_that_projects_file(),
        org_capture_template_routes_a_source_file_todo_to_its_project(),
        single_file_capture_creates_a_linked_project_heading_and_child_todo(),
    ]
}
