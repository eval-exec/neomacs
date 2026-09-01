use expect_test::expect;

use super::ParityBatchCase;

fn the_pinned_request_package_no_longer_ships_the_library_airplay_requires() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_pinned_request_package_no_longer_ships_the_library_airplay_requires",
        r##"
(list
 ;; What the package asks for, from its own header.
 :declared-requirements
 '((request "20130110.2144") (simple-httpd "1.4.1") (deferred "0.3.1"))
 ;; What the pinned closure actually provides.  Every declared dependency
 ;; is present; the library `airplay.el' requires on its first line is not.
 :on-the-load-path
 (mapcar (lambda (library) (cons library (and (locate-library library) t)))
         '("request" "simple-httpd" "deferred" "request-deferred"))
 ;; `request-deferred' was part of the request package when airplay was
 ;; written in 2013 and has since been dropped from it.  The pinned build
 ;; ships these files and no others.
 :files-in-the-pinned-request
 (sort (mapcar #'file-name-nondirectory
               (directory-files
                (file-name-directory (locate-library "request")) t "\\.el\\'"))
       #'string<)
 ;; So requiring the package fails, and this is the exact failure.
 :requiring-it (airplay-test-load))
"##,
        expect![[
            r#"OK (:declared-requirements ((request "20130110.2144") (simple-httpd "1.4.1") (deferred "0.3.1")) :on-the-load-path (("request" . t) ("simple-httpd" . t) ("deferred" . t) ("request-deferred")) :files-in-the-pinned-request ("request-autoloads.el" "request-pkg.el" "request.el") :requiring-it (file-missing "Cannot open load file" "No such file or directory" "request-deferred"))"#
        ]],
    )
}

fn installing_the_package_gives_commands_that_exist_but_cannot_run() -> ParityBatchCase {
    ParityBatchCase::value(
        "installing_the_package_gives_commands_that_exist_but_cannot_run",
        r##"
(list
 ;; `package-initialize' loads the autoloads, which succeed, so every
 ;; command is defined and interactive before anything else happens.
 :defined-by-the-autoloads
 (mapcar (lambda (command)
           (list command
                 :fboundp (and (fboundp command) t)
                 :interactive (and (commandp command) t)
                 :autoload (eq (car-safe (indirect-function command)) 'autoload)))
         airplay-test-commands)
 ;; The package is not loaded, and `M-x' will happily offer all of them.
 :feature-present (and (featurep 'airplay) t)
 :offered-by-completion
 (length (all-completions "airplay" obarray #'commandp)))
"##,
        expect![
            "OK (:defined-by-the-autoloads ((airplay/image:view :fboundp t :interactive nil :autoload t) (airplay:stop :fboundp t :interactive t :autoload t) (airplay/video:play :fboundp t :interactive nil :autoload t) (airplay/video:scrub :fboundp t :interactive nil :autoload t) (airplay/video:seek :fboundp t :interactive nil :autoload t) (airplay/video:info :fboundp t :interactive nil :autoload t) (airplay/video:pause :fboundp t :interactive t :autoload t) (airplay/video:resume :fboundp t :interactive t :autoload t)) :feature-present nil :offered-by-completion 3)"
        ],
    )
}

fn invoking_any_of_those_commands_fails_the_same_way() -> ParityBatchCase {
    ParityBatchCase::value(
        "invoking_any_of_those_commands_fails_the_same_way",
        r##"
(mapcar
 (lambda (command)
   ;; Calling an autoloaded command loads its file, which is where the
   ;; missing library is required.  Every one of them fails identically,
   ;; and none of them reaches any airplay code at all.
   (cons command
         (condition-case error
             (progn (call-interactively command) :ran)
           (error (airplay-test-plain error)))))
 airplay-test-commands)
"##,
        expect![[
            r#"OK ((airplay/image:view file-missing "Cannot open load file" "No such file or directory" "request-deferred") (airplay:stop file-missing "Cannot open load file" "No such file or directory" "request-deferred") (airplay/video:play file-missing "Cannot open load file" "No such file or directory" "request-deferred") (airplay/video:scrub file-missing "Cannot open load file" "No such file or directory" "request-deferred") (airplay/video:seek file-missing "Cannot open load file" "No such file or directory" "request-deferred") (airplay/video:info file-missing "Cannot open load file" "No such file or directory" "request-deferred") (airplay/video:pause file-missing "Cannot open load file" "No such file or directory" "request-deferred") (airplay/video:resume file-missing "Cannot open load file" "No such file or directory" "request-deferred"))"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        the_pinned_request_package_no_longer_ships_the_library_airplay_requires(),
        installing_the_package_gives_commands_that_exist_but_cannot_run(),
        invoking_any_of_those_commands_fails_the_same_way(),
    ]
}
