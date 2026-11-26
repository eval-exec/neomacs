;;; startup-command-line-font.el --- Explicit font at first native allocation -*- lexical-binding: t -*-

(run-at-time
 0.2 nil
 (lambda ()
   (condition-case err
       (progn
         (unless (and (equal (face-attribute 'default :family) "DejaVu Sans Mono")
                      (= (frame-char-width) 13) (= (frame-char-height) 25)
                      (= (frame-width) 80))
           (error "Command-line font geometry is incoherent: family=%S cell=%sx%s columns=%s parameters=%S"
                  (face-attribute 'default :family) (frame-char-width)
                  (frame-char-height) (frame-width) (frame-parameters)))
         ;; Exercise the same public font-info control as native resources.
         (let ((actual (font-info (face-attribute 'default :font)))
               (control (font-info "DejaVu Sans Mono 16")))
           (message "STARTUP-FONT native=%sx%s cell=%sx%s actual=%S control=%S parameters=%S"
                    (frame-native-width) (frame-native-height)
                    (frame-char-width) (frame-char-height) actual control (frame-parameters))
           (unless (and (vectorp actual) (vectorp control)
                        (= (aref actual 2) (aref control 2)))
             (error "Startup font size differs from named control: %S vs %S" actual control)))
         ;; GNU's Gtk+ native height excludes the external menu and tool
         ;; bars (src/frame.c:4401-4418); the outer measures are the host
         ;; window the toolkit allocated.  Record both so the test checks the
         ;; allocation and the GNU-correct native frame separately.
         (with-temp-file (getenv "NEOMACS_GUI_STATE_JSON")
           (insert (json-serialize `((native_width . ,(frame-native-width))
                                     (native_height . ,(frame-native-height))
                                     (outer_width . ,(frame-outer-width))
                                     (outer_height . ,(frame-outer-height))))))
         (kill-emacs 0))
     (error (message "Command-line startup font: %S" err) (kill-emacs 1)))))
