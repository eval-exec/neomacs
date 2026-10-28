use crate::common;

use common::{run_neovm_eval, run_oracle_eval};

struct FaceCase {
    name: &'static str,
    form: &'static str,
}

#[test]
fn compat_face_semantics_matches_gnu_emacs() {
    let cases = [
        FaceCase {
            name: "face_attributes_as_vector_permissive_merge",
            form: r#"(list
  (face-attributes-as-vector nil)
  (face-attributes-as-vector '(:foreground "red" :background "blue" :weight bold :underline (:style wave :color "red") :box t :extend t))
  (face-attributes-as-vector '(:bogus 1 :foreground "red" :font foo :inherit bar :fontset baz :stipple qux))
  (face-attributes-as-vector 1))"#,
        },
        FaceCase {
            name: "face_attributes_as_vector_nil_and_box_cases",
            form: r#"(list
  (face-attributes-as-vector '(:underline nil :overline nil :strike-through nil :box nil))
  (face-attributes-as-vector '(:box "red"))
  (face-attributes-as-vector '(:box (:line-width 3 :color "red" :style released-button)))
  (face-attributes-as-vector '(:foreground nil :background nil :distant-foreground nil :extend nil)))"#,
        },
        FaceCase {
            name: "frame_sensitive_face_state",
            form: r##"(let* ((f (selected-frame))
       (face 'compat-runtime-face))
  (list
   (vectorp (internal-make-lisp-face face f))
   (eq (internal-copy-lisp-face 'default face f f) face)
   (eq (internal-set-lisp-face-attribute face :foreground "red" f) face)
   (equal (internal-get-lisp-face-attribute face :foreground f) "red")
   (progn
     (internal-set-lisp-face-attribute face :foreground "blue" t)
     (internal-merge-in-global-face face f)
     (equal (internal-get-lisp-face-attribute face :foreground f) "blue"))))"##,
        },
        FaceCase {
            name: "obsolete_face_aliases_resolve_through_public_face_operations",
            form: r#"(progn
  (defface compat-face-alias-target
    '((t :foreground "red"))
    "Oracle target face.")
  (define-obsolete-face-alias
    'compat-face-alias-intermediate
    'compat-face-alias-target
    "1.0")
  (define-obsolete-face-alias
    'compat-face-alias
    'compat-face-alias-intermediate
    "1.0")
  (list
   (and (facep 'compat-face-alias) t)
   (get 'compat-face-alias 'face-alias)
   (face-attribute 'compat-face-alias :foreground nil t)
   (face-equal 'compat-face-alias 'compat-face-alias-target)
   (face-font 'compat-face-alias)
   (progn
     (set-face-attribute 'compat-face-alias nil :background "blue")
     (face-attribute 'compat-face-alias-target :background nil t))
   (face-differs-from-default-p 'compat-face-alias)))"#,
        },
        FaceCase {
            name: "circular_face_aliases_signal",
            form: r#"(progn
  (put 'compat-circular-face-alias-a
       'face-alias
       'compat-circular-face-alias-b)
  (put 'compat-circular-face-alias-b
       'face-alias
       'compat-circular-face-alias-a)
  (condition-case err
      (facep 'compat-circular-face-alias-a)
    (error err)))"#,
        },
    ];

    // GNU's `face-font` has two implementations selected at build time by
    // HAVE_WINDOW_SYSTEM (src/xfaces.c:4458-4473): a build with a window
    // system reports the realized face's font (nil for a face on a tty
    // frame), while a build without one returns the literal "tty".  The
    // remote Mac oracle is configured --without-x --without-ns, so it takes
    // the literal branch; this port always compiles its GUI backend.  Ask the
    // oracle which shape it is once, so the comparison still compares like
    // with like.
    // Every GUI build defines x-create-frame (X/GTK, PGTK and NS share the
    // DEFUN: src/xfns.c:4916, src/nsfns.m:1192); w32/haiku/android use their
    // own names.  A tty-only build defines none of them.
    let oracle_has_window_system = run_oracle_eval(
        "(prin1 (or (fboundp 'x-create-frame) (fboundp 'w32-create-frame) \
                    (fboundp 'haiku-create-frame) (fboundp 'android-create-frame)))",
    )
    .map(|answer| answer.contains('t'))
    .unwrap_or(false);

    for case in cases {
        let gnu = run_oracle_eval(case.form).expect("GNU Emacs evaluation");
        let neovm = run_neovm_eval(case.form).expect("NeoVM evaluation");
        if !oracle_has_window_system
            && case.name == "obsolete_face_aliases_resolve_through_public_face_operations"
        {
            // Only the `face-font' element may differ, and only because the
            // oracle took xfaces.c:4468-4473's no-window-system branch.
            assert!(
                gnu.contains("\"tty\""),
                "a window-system-less GNU must answer \"tty\": {gnu}"
            );
            assert_eq!(
                gnu.replace("\"tty\"", "nil"),
                neovm,
                "face semantics mismatch beyond the documented build-shape \
                 branch for {}:\nGNU: {}\nNeoVM: {}",
                case.name,
                gnu,
                neovm
            );
            continue;
        }
        assert_eq!(
            neovm, gnu,
            "face semantics mismatch for {}:\nGNU: {}\nNeoVM: {}",
            case.name, gnu, neovm
        );
    }
}
