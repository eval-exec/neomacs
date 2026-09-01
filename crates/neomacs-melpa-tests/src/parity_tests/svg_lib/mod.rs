use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, SVG_LIB_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const SVG_LIB_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const SVG_LIB_TEST_PRELUDE: &str = r#####"
(require 'cl-lib)
(require 'svg-lib)

(defun svg-lib-parity-font-info (&rest _arguments)
  (let ((info (make-vector 12 nil)))
    (aset info 2 12)
    (aset info 3 16)
    (aset info 8 12)
    (aset info 11 8)
    info))

(defun svg-lib-parity-create-image (data type data-p &rest properties)
  (append (list 'image :type type :data data :data-p data-p)
          properties))

(defun svg-lib-parity-with-display (function)
  (cl-letf (((symbol-function 'image-type-available-p)
             (lambda (type) (eq type 'svg)))
            ((symbol-function 'create-image)
             #'svg-lib-parity-create-image)
            ((symbol-function 'window-font-width)
             (lambda (&optional _window) 8))
            ((symbol-function 'window-font-height)
             (lambda (&optional _window) 16))
            ((symbol-function 'font-info)
             #'svg-lib-parity-font-info))
    (funcall function)))

(defun svg-lib-parity-image-data (image)
  (plist-get (cdr image) :data))

(defun svg-lib-parity-image-dom (image)
  (with-temp-buffer
    (insert (svg-lib-parity-image-data image))
    (car (xml-parse-region (point-min) (point-max)))))

(defun svg-lib-parity-dom-shape (dom)
  (cons (xml-node-name dom)
        (cons (xml-node-attributes dom)
              (mapcar
               (lambda (child)
                 (if (stringp child)
                     child
                   (svg-lib-parity-dom-shape child)))
               (xml-node-children dom)))))

(defun svg-lib-parity-widget-summary (image)
  (let* ((dom (svg-lib-parity-image-dom image))
         (attributes (xml-node-attributes dom)))
    (list
     :size (list (cdr (assq 'width attributes))
                 (cdr (assq 'height attributes)))
     :children
     (mapcar #'svg-lib-parity-dom-shape
             (seq-filter (lambda (child) (not (stringp child)))
                         (xml-node-children dom))))))
"#####;

fn svg_lib_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(SVG_LIB_MELPA_PIN, "svg-lib.el")
        .expect("prepare pinned svg-lib source below ./tmp")
        .with_prelude(SVG_LIB_TEST_PRELUDE)
        .with_timeout(SVG_LIB_TEST_TIMEOUT)
}

fn themed_style_inheritance_normalizes_colors_weights_and_explicit_nil() -> ParityBatchCase {
    let elisp_form = r#####"
(let* ((svg-lib-style-default
        '(:background "black"
          :foreground "white"
          :padding 1
          :margin 0
          :stroke 2
          :radius 3
          :alignment 0.5
          :width 20
          :height 0.9
          :scale 0.75
          :ascent center
          :crop-left nil
          :crop-right nil
          :collection "material"
          :font-family "Parity Mono"
          :font-size 12
          :font-weight regular))
       (release
        (svg-lib-style nil
                       :background "navy"
                       :foreground "red"
                       :padding 3
                       :margin 1
                       :font-weight 'semibold))
       (muted
        (svg-lib-style release
                       :background nil
                       :foreground "brand-accent"
                       :stroke 0
                       :font-weight 550
                       :crop-left t)))
  (list
   :release release
   :muted muted
   :base-unchanged svg-lib-style-default
   :individual-colors
   (mapcar #'svg-lib-convert-color
           '(nil "red" "#123456" "brand-accent"))))
"#####;
    let expect = expect![[
        r####"OK (:release (:background "#0000ff" :foreground "#ff0000" :padding 3 :margin 1 :stroke 2 :radius 3 :alignment 0.5 :width 20 :height 0.9 :scale 0.75 :ascent center :crop-left nil :crop-right nil :collection "material" :font-family "Parity Mono" :font-size 12 :font-weight 600) :muted (:background nil :foreground "brand-accent" :padding 3 :margin 1 :stroke 0 :radius 3 :alignment 0.5 :width 20 :height 0.9 :scale 0.75 :ascent center :crop-left t :crop-right nil :collection "material" :font-family "Parity Mono" :font-size 12 :font-weight 550) :base-unchanged (:background "black" :foreground "white" :padding 1 :margin 0 :stroke 2 :radius 3 :alignment 0.5 :width 20 :height 0.9 :scale 0.75 :ascent center :crop-left nil :crop-right nil :collection "material" :font-family "Parity Mono" :font-size 12 :font-weight regular) :individual-colors (nil "#ff0000" "#0000ff" "brand-accent"))"####
    ]];
    ParityBatchCase::value(
        "themed_style_inheritance_normalizes_colors_weights_and_explicit_nil",
        elisp_form,
        expect,
    )
}

fn release_dashboard_builds_strict_tags_bars_and_pies() -> ParityBatchCase {
    let elisp_form = r#####"
(svg-lib-parity-with-display
 (lambda ()
   (let* ((style
           '(:background "#ddeeff"
             :foreground "#112233"
             :padding 2
             :margin 1
             :stroke 2
             :radius 4
             :alignment 0.25
             :width 10
             :height 1.0
             :scale 0.75
             :ascent 80
             :crop-left nil
             :crop-right nil
             :collection "fixture"
             :font-family "Parity Mono"
             :font-size 12
             :font-weight 600))
          (tag (svg-lib-tag "READY" style))
          (cropped
           (svg-lib-tag "NEXT" style
                        :crop-left t
                        :crop-right t
                        :alignment 0.75
                        :stroke 1))
          (bar-empty (svg-lib-progress-bar 0.0 style))
          (bar-partial
           (svg-lib-progress-bar 0.375 style
                                 :width 12
                                 :padding 1
                                 :radius 2))
          (bar-full (svg-lib-progress-bar 1.0 style))
          (pie-quarter (svg-lib-progress-pie 0.25 style))
          (pie-complete (svg-lib-progress-pie 1.0 style)))
     (list
      :tag (svg-lib-parity-widget-summary tag)
      :cropped (svg-lib-parity-widget-summary cropped)
      :bars (mapcar #'svg-lib-parity-widget-summary
                    (list bar-empty bar-partial bar-full))
      :pies (mapcar #'svg-lib-parity-widget-summary
                    (list pie-quarter pie-complete))
      :image-props
      (mapcar (lambda (image)
                (list (plist-get (cdr image) :type)
                      (plist-get (cdr image) :data-p)
                      (plist-get (cdr image) :ascent)))
              (list tag cropped bar-partial pie-quarter))))))
"#####;
    let expect = expect![[
        r####"OK (:tag (:size ("64" "16.0") :children ((rect ((width . "56") (height . "16.0") (x . "2.0") (y . "0") (rx . "4") (fill . "#0000ff"))) (rect ((width . "54") (height . "14.0") (x . "3.0") (y . "1.0") (rx . "3.0") (fill . "#ffffff"))) (text ((y . "12") (x . "10.0") (fill . "#0000ff") (font-size . "12") (font-weight . "600") (font-family . "Parity Mono")) "READY"))) :cropped (:size ("56" "16.0") :children ((rect ((width . "64") (height . "16.0") (x . "-2.0") (y . "0") (rx . "4") (fill . "#0000ff"))) (rect ((width . "63") (height . "15.0") (x . "-1.5") (y . "0.5") (rx . "3.5") (fill . "#ffffff"))) (text ((y . "12") (x . "14.0") (fill . "#0000ff") (font-size . "12") (font-weight . "600") (font-family . "Parity Mono")) "NEXT"))) :bars ((:size ("88" "16.0") :children ((rect ((width . "80") (height . "16.0") (x . "4") (y . "0") (rx . "4") (fill . "#0000ff"))) (rect ((width . "78") (height . "14.0") (x . "5.0") (y . "1.0") (rx . "3.0") (fill . "#ffffff"))) (rect ((width . "-6.0") (height . "10.0") (x . "7.0") (y . "3.0") (rx . "3.0") (fill . "#0000ff"))))) (:size ("104" "16.0") :children ((rect ((width . "96") (height . "16.0") (x . "4") (y . "0") (rx . "2") (fill . "#0000ff"))) (rect ((width . "94") (height . "14.0") (x . "5.0") (y . "1.0") (rx . "1.0") (fill . "#ffffff"))) (rect ((width . "32.0") (height . "12.0") (x . "6.0") (y . "2.0") (rx . "1.0") (fill . "#0000ff"))))) (:size ("88" "16.0") :children ((rect ((width . "80") (height . "16.0") (x . "4") (y . "0") (rx . "4") (fill . "#0000ff"))) (rect ((width . "78") (height . "14.0") (x . "5.0") (y . "1.0") (rx . "3.0") (fill . "#ffffff"))) (rect ((width . "74.0") (height . "10.0") (x . "7.0") (y . "3.0") (rx . "3.0") (fill . "#0000ff")))))) :pies ((:size ("24" "16.0") :children ((circle ((cx . "12") (cy . "8.0") (r . "7.0") (fill . "#0000ff"))) (circle ((cx . "12") (cy . "8.0") (r . "6.0") (fill . "#ffffff"))) (path ((d . "M 12 8.0 L 12.0 4.0 A 4.0 4.0 0 0 1 16.0 8.0") (fill . "#0000ff"))))) (:size ("24" "16.0") :children ((circle ((cx . "12") (cy . "8.0") (r . "7.0") (fill . "#0000ff"))) (circle ((cx . "12") (cy . "8.0") (r . "6.0") (fill . "#ffffff"))) (circle ((cx . "12") (cy . "8.0") (r . "4.0") (fill . "#0000ff")))))) :image-props ((svg t 80) (svg t 80) (svg t 80) (svg t 80)))"####
    ]];
    ParityBatchCase::value(
        "release_dashboard_builds_strict_tags_bars_and_pies",
        elisp_form,
        expect,
    )
}

fn cached_remote_icon_drives_icon_and_icon_tag_rendering_without_a_second_fetch() -> ParityBatchCase
{
    let elisp_form = r#####"
(svg-lib-parity-with-display
 (lambda ()
   (let* ((svg-lib-icons-dir (make-temp-file "svg-lib-parity-icons-" t))
          (svg-lib-icon-collections
           '(("fixture" . "https://fixture.invalid/%s.svg")))
          (style
           '(:background "#ffffff"
             :foreground "#0055aa"
             :padding 1
             :margin 0
             :stroke 1
             :radius 2
             :alignment 0.5
             :width 8
             :height 1.0
             :scale 0.5
             :ascent center
             :crop-left nil
             :crop-right nil
             :collection "fixture"
             :font-family "Parity Mono"
             :font-size 12
             :font-weight 600))
          (response-buffers nil)
          (fetches 0)
          first second tagged result)
     (unwind-protect
         (cl-letf
             (((symbol-function 'url-retrieve-synchronously)
               (lambda (url &rest _arguments)
                 (setq fetches (1+ fetches))
                 (let ((buffer (generate-new-buffer " *svg-lib fixture response*")))
                   (push buffer response-buffers)
                   (with-current-buffer buffer
                     (insert
                      "HTTP/1.1 200 OK\nContent-Type: image/svg+xml\n\n"
                      "<svg viewBox=\"0 0 24 24\">"
                      "<path d=\"M2 12h20\"/>"
                      "<path d=\"M12 2v20\" fill=\"none\"/>"
                      "</svg>"))
                   buffer))))
           (setq first (svg-lib-icon "deploy" style)
                 second (svg-lib-icon "deploy" style)
                 tagged (svg-lib-icon+tag "deploy" "SHIP" style))
           (let* ((first-dom (svg-lib-parity-image-dom first))
                  (tagged-dom (svg-lib-parity-image-dom tagged))
                  (cached-file
                   (expand-file-name "fixture_deploy.svg" svg-lib-icons-dir)))
             (setq result
                   (list
                    :fetches fetches
                    :cache-files
                    (sort (directory-files svg-lib-icons-dir nil "\\.svg\\'")
                          #'string-lessp)
                    :cache-contents
                    (with-temp-buffer
                      (insert-file-contents cached-file)
                      (buffer-string))
                    :same-generated-data
                    (equal (svg-lib-parity-image-data first)
                           (svg-lib-parity-image-data second))
                    :icon
                    (list
                     (xml-node-attributes first-dom)
                     (mapcar #'xml-node-attributes
                             (xml-get-children first-dom 'rect))
                     (mapcar #'xml-node-attributes
                             (xml-get-children first-dom 'path)))
                    :tagged
                    (list
                     (xml-node-attributes tagged-dom)
                     (mapcar #'xml-node-attributes
                             (xml-get-children tagged-dom 'text))
                     (mapcar #'xml-node-attributes
                             (xml-get-children tagged-dom 'path)))))))
       (dolist (buffer response-buffers)
         (when (buffer-live-p buffer) (kill-buffer buffer)))
       (when (file-directory-p svg-lib-icons-dir)
         (delete-directory svg-lib-icons-dir t)))
     result)))
"#####;
    let expect = expect![[
        r####"OK (:fetches 1 :cache-files ("fixture_deploy.svg") :cache-contents "<svg viewBox=\"0 0 24 24\"><path d=\"M2 12h20\"/><path d=\"M12 2v20\" fill=\"none\"/></svg>" :same-generated-data t :icon (((width . "24") (height . "16.0") (version . "1.1") (xmlns . "http://www.w3.org/2000/svg") (xmlns:xlink . "http://www.w3.org/1999/xlink")) (((width . "24") (height . "16.0") (x . "0") (y . "0") (rx . "2") (fill . "#0000ff")) ((width . "23") (height . "15.0") (x . "0.5") (y . "0.5") (rx . "1.5") (fill . "#ffffff"))) (((transform . "translate(0.000000,0.000000) scale(0.333333) translate(24.000000,12.000000)") (fill . "#0000ff") (d . "M2 12h20")) ((transform . "translate(0.000000,0.000000) scale(0.333333) translate(24.000000,12.000000)") (fill . "#0000ff") (d . "M12 2v20")))) :tagged (((width . "56") (height . "16.0") (version . "1.1") (xmlns . "http://www.w3.org/2000/svg") (xmlns:xlink . "http://www.w3.org/1999/xlink")) (((y . "12") (x . "20") (fill . "#0000ff") (font-size . "12") (font-weight . "600") (font-family . "Parity Mono"))) (((transform . "translate(0.000000,0.000000) scale(0.333333) translate(18.000000,12.000000)") (fill . "#0000ff") (d . "M2 12h20")) ((transform . "translate(0.000000,0.000000) scale(0.333333) translate(18.000000,12.000000)") (fill . "#0000ff") (d . "M12 2v20")))))"####
    ]];
    ParityBatchCase::value(
        "cached_remote_icon_drives_icon_and_icon_tag_rendering_without_a_second_fetch",
        elisp_form,
        expect,
    )
}

fn composed_status_strip_preserves_children_dimensions_and_translation() -> ParityBatchCase {
    let elisp_form = r#####"
(svg-lib-parity-with-display
 (lambda ()
   (let* ((style
           '(:background "#202020"
             :foreground "#f0c000"
             :padding 1
             :margin 0
             :stroke 1
             :radius 2
             :alignment 0.5
             :width 6
             :height 1.0
             :scale 0.75
             :ascent center
             :crop-left nil
             :crop-right nil
             :collection "fixture"
             :font-family "Parity Mono"
             :font-size 12
             :font-weight 600))
          (left (svg-lib-tag "BUILD" style :crop-right t))
          (right
           (svg-lib-progress-bar 0.625 style
                                 :width 7
                                 :padding 1
                                 :crop-left t))
          (combined (svg-lib-concat left right))
          (children (seq-filter (lambda (child) (not (stringp child)))
                                (xml-node-children combined))))
     (list
      :size
      (let ((attributes (xml-node-attributes combined)))
        (list (cdr (assq 'width attributes))
              (cdr (assq 'height attributes))))
      :children (mapcar #'svg-lib-parity-dom-shape children)))))
"#####;
    let expect = expect![[
        r####"OK (:size (104 16.0) :children ((rect ((width . "56") (height . "16.0") (x . "0.0") (y . "0") (rx . "2") (fill . "#ffff00"))) (rect ((width . "55") (height . "15.0") (x . "0.5") (y . "0.5") (rx . "1.5") (fill . "#000000"))) (text ((y . "12") (x . "4.0") (fill . "#ffff00") (font-size . "12") (font-weight . "600") (font-family . "Parity Mono")) "BUILD") (rect ((transform . "translate(48.000000,0)") (width . "56") (height . "16.0") (x . "0") (y . "0") (rx . "2") (fill . "#ffff00"))) (rect ((transform . "translate(48.000000,0)") (width . "55") (height . "15.0") (x . "0.5") (y . "0.5") (rx . "1.5") (fill . "#000000"))) (rect ((transform . "translate(48.000000,0)") (width . "32.0") (height . "13.0") (x . "1.5") (y . "1.5") (rx . "1.5") (fill . "#ffff00")))))"####
    ]];
    ParityBatchCase::value(
        "composed_status_strip_preserves_children_dimensions_and_translation",
        elisp_form,
        expect,
    )
}

fn fixed_release_dates_build_calendar_week_and_day_badges() -> ParityBatchCase {
    let elisp_form = r#####"
(svg-lib-parity-with-display
 (lambda ()
   (let* ((system-time-locale "C")
          (date (encode-time 0 30 12 29 2 2024 0))
          (style
           '(:background "#fff8e1"
             :foreground "#ff6f00"
             :padding 1
             :margin 1
             :stroke 2
             :radius 5
             :alignment 0.5
             :width 5
             :height 2
             :scale 0.75
             :ascent center
             :crop-left nil
             :crop-right nil
             :collection "fixture"
             :font-family "Parity Mono"
             :font-size 12
             :font-weight 600))
          (calendar (svg-lib-date date style))
          (week (svg-lib-week-date date style))
          (day (svg-lib-day-date date style)))
     (let* ((doms (mapcar #'svg-lib-parity-image-dom
                          (list calendar week day)))
            (geometries
             (mapcar
              (lambda (dom)
                (list
                 (xml-node-attributes dom)
                 (mapcar #'xml-node-attributes
                         (xml-get-children dom 'rect))))
              doms)))
       (list
        :geometry (car geometries)
        :all-share-geometry
        (cl-every (lambda (geometry) (equal geometry (car geometries)))
                  (cdr geometries))
        :texts
        (mapcar
         (lambda (dom)
           (mapcar (lambda (node)
                     (list (xml-node-attributes node)
                           (car (xml-node-children node))))
                   (xml-get-children dom 'text)))
         doms))))))
"#####;
    let expect = expect![[
        r####"OK (:geometry (((width . "48") (height . "32") (version . "1.1") (xmlns . "http://www.w3.org/2000/svg") (xmlns:xlink . "http://www.w3.org/1999/xlink")) (((width . "40") (height . "32") (x . "4") (y . "0") (rx . "5") (fill . "#ff0000")) ((width . "38") (height . "30") (x . "5.0") (y . "1.0") (rx . "4.0") (fill . "#ffffff")) ((width . "38") (height . "14") (x . "5.0") (y . "1.0") (rx . "4.0") (fill . "#ff0000")) ((width . "38") (height . "14") (x . "5.0") (y . "11.0") (rx . "0") (fill . "#ffffff")))) :all-share-geometry t :texts (((((y . "+0.95em") (x . "24") (text-anchor . "middle") (fill . "#ffffff") (font-size . "10.8") (font-weight . "bold") (font-family . "Parity Mono")) "FEB") (((y . "+1.6em") (x . "24") (text-anchor . "middle") (fill . "#ff0000") (font-size . "20.4") (font-weight . "bold") (font-family . "Parity Mono")) "29")) ((((y . "+0.95em") (x . "24") (text-anchor . "middle") (fill . "#ffffff") (font-size . "10.8") (font-weight . "bold") (font-family . "Parity Mono")) "WEEK") (((y . "+1.6em") (x . "24") (text-anchor . "middle") (fill . "#ff0000") (font-size . "20.4") (font-weight . "bold") (font-family . "Parity Mono")) "09")) ((((y . "+0.95em") (x . "24") (text-anchor . "middle") (fill . "#ffffff") (font-size . "10.8") (font-weight . "bold") (font-family . "Parity Mono")) "THU") (((y . "+1.6em") (x . "24") (text-anchor . "middle") (fill . "#ff0000") (font-size . "20.4") (font-weight . "bold") (font-family . "Parity Mono")) "29"))))"####
    ]];
    ParityBatchCase::value(
        "fixed_release_dates_build_calendar_week_and_day_badges",
        elisp_form,
        expect,
    )
}

fn interactive_buttons_keep_regions_keymaps_states_hooks_and_mode_cleanup() -> ParityBatchCase {
    let elisp_form = r#####"
(svg-lib-parity-with-display
 (lambda ()
   (with-temp-buffer
     (let ((svg-lib-button--id-counter 40)
           (font-lock-extra-managed-props '(face display keymap help-echo))
           (yank-excluded-properties '(display keymap help-echo rear-nonsticky))
           (hook-events nil)
           (tooltip-events nil)
           (advice-events nil))
       (cl-letf
           (((symbol-function 'face-attribute)
             (lambda (face attribute &rest _arguments)
               (pcase attribute
                 (:box '(:line-width 1))
                 (:family "Parity Mono")
                 (:weight (pcase face
                            ('svg-lib-button-active-face 'regular)
                            ('svg-lib-button-hover-face 'semibold)
                            (_ 'bold)))
                 (_ nil))))
            ((symbol-function 'face-foreground)
             (lambda (face &rest _arguments)
               (if (eq face 'svg-lib-button-hover-face)
                   "#ffffff"
                 "#112233")))
            ((symbol-function 'face-background)
             (lambda (face &rest _arguments)
               (if (eq face 'svg-lib-button-hover-face)
                   "#0055aa"
                 "#ddeeff")))
            ((symbol-function 'tooltip-mode)
             (lambda (argument) (push argument tooltip-events)))
            ((symbol-function 'advice-add)
             (lambda (symbol where function &rest _properties)
               (push (list :add symbol where function) advice-events)))
            ((symbol-function 'advice-remove)
             (lambda (symbol function)
               (push (list :remove symbol function) advice-events))))
         (svg-lib-button-mode 1)
         (let* ((first
                 (svg-lib-button
                  "Deploy"
                  (lambda () (push :deploy hook-events))
                  "Deploy release"))
                (second
                 (svg-lib-button
                  "Rollback"
                  (lambda () (push :rollback hook-events))
                  "Rollback release")))
           (insert first "|" second)
           (let* ((first-id (get-text-property 1 'button-id))
                  (second-pos (+ 2 (length first)))
                  (second-id (get-text-property second-pos 'button-id))
                  (first-region (svg-lib-button--search first-id))
                  (first-list (get-text-property 1 'button-list))
                  (first-map (get-text-property 1 'keymap))
                  (initial
                   (list
                    :text (buffer-substring-no-properties
                           (point-min) (point-max))
                    :ids (list first-id second-id)
                    :region first-region
                    :at-points
                    (list (svg-lib-button--at-point 1)
                          (svg-lib-button--at-point second-pos)
                          (svg-lib-button--at-point (1- second-pos)))
                    :states
                    (list (svg-lib-button--get-state first-id)
                          (svg-lib-button--get-state second-id))
                    :commands
                    (mapcar (lambda (key) (lookup-key first-map key))
                            '([down-mouse-1] [mouse-1] [drag-mouse-1]))
                    :properties
                    (list
                     (get-text-property 1 'svg-lib-button)
                     (get-text-property 1 'pointer)
                     (get-text-property 1 'front-sticky)
                     (get-text-property 1 'rear-nonsticky)
                     (functionp (get-text-property 1 'help-echo))))))
             (svg-lib-button--set-state first-id 'hover)
             (let ((hover
                    (list
                     (svg-lib-button--get-state first-id)
                     svg-lib-button--hover-id
                     (eq (get-text-property 1 'display)
                         (cdr (assq 'hover first-list))))))
               (svg-lib-button--set-state first-id 'press)
               (let ((press
                      (list
                       (svg-lib-button--get-state first-id)
                       svg-lib-button--press-id
                       (eq (get-text-property 1 'display)
                           (cdr (assq 'press first-list))))))
                 (svg-lib-button--set-state second-id 'hover)
                 (funcall (get-text-property 1 'button-hook))
                 (funcall (get-text-property second-pos 'button-hook))
                 (svg-lib-button-mode -1)
                 (list
                  :initial initial
                  :hover hover
                  :press press
                  :after-second-hover
                  (list (svg-lib-button--get-state first-id)
                        (svg-lib-button--get-state second-id)
                        svg-lib-button--hover-id)
                  :hooks (nreverse hook-events)
                  :mode
                  (list svg-lib-button-mode
                        font-lock-extra-managed-props
                        yank-excluded-properties
                        (nreverse tooltip-events)
                        (nreverse advice-events))))))))))))
"#####;
    let expect = expect![[
        r####"OK (:initial (:text "Deploy |Rollback " :ids (41 42) :region (1 . 8) :at-points (41 42 nil) :states (active active) :commands (svg-lib-button--mouse-down svg-lib-button--mouse-press svg-lib-button--mouse-drag) :properties (t hand nil t t)) :hover (hover 41 t) :press (press 41 t) :after-second-hover (press hover 42) :hooks (:deploy :rollback) :mode (nil (face) (display rear-nonsticky) (1) ((:add remove-text-properties :around svg-lib-button--remove-text-properties) (:remove remove-text-properties svg-lib-button--remove-text-properties))))"####
    ]];
    ParityBatchCase::value(
        "interactive_buttons_keep_regions_keymaps_states_hooks_and_mode_cleanup",
        elisp_form,
        expect,
    )
}

fn unavailable_svg_support_reports_the_package_error_before_image_creation() -> ParityBatchCase {
    let elisp_form = r#####"
(let ((create-calls 0))
  (cl-letf (((symbol-function 'image-type-available-p)
             (lambda (_type) nil))
            ((symbol-function 'create-image)
             (lambda (&rest _arguments)
               (setq create-calls (1+ create-calls))))
            ((symbol-function 'window-font-width)
             (lambda (&optional _window) 8))
            ((symbol-function 'window-font-height)
             (lambda (&optional _window) 16))
            ((symbol-function 'font-info)
             #'svg-lib-parity-font-info))
    (list
     (condition-case error
         (svg-lib-tag "NO SVG" nil
                      :font-family "Parity Mono"
                      :font-size 12)
       (error (list (car error) (cadr error))))
     :create-calls create-calls)))
"#####;
    let expect = expect![[
        r####"OK ((error "svg-lib.el requires Emacs to be compiled with svg support.\n") :create-calls 0)"####
    ]];
    ParityBatchCase::value(
        "unavailable_svg_support_reports_the_package_error_before_image_creation",
        elisp_form,
        expect,
    )
}

#[test]
fn svg_lib_package_batch() {
    let cases = vec![
        themed_style_inheritance_normalizes_colors_weights_and_explicit_nil(),
        release_dashboard_builds_strict_tags_bars_and_pies(),
        cached_remote_icon_drives_icon_and_icon_tag_rendering_without_a_second_fetch(),
        composed_status_strip_preserves_children_dimensions_and_translation(),
        fixed_release_dates_build_calendar_week_and_day_badges(),
        interactive_buttons_keep_regions_keymaps_states_hooks_and_mode_cleanup(),
        unavailable_svg_support_reports_the_package_error_before_image_creation(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed svg-lib parity test");
    assert_oracle_batch_cases(svg_lib_oracle(), test_name, "svg_lib_parity", &cases);
}
