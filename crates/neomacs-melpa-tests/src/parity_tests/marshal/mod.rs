//! Practical parity for Marshal's public EIEIO serialization interfaces.

use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, MARSHAL_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(120);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'ht)
(require 'marshal)
(set-window-configuration (current-window-configuration))

(defconst marshal422-test-tree
  "0e52f8ef6216f21d14bf67e9162140491d7f10cb")
(defconst marshal422-test-manifest
  '(("marshal-pkg.el" . "ad5c070d6b5ee4823b04fc7e46c6d88738e508d49eddaa6e6e1199535ad6ed49")
    ("marshal.el" . "b1056794b254eaa2ab917534843abfa37b6d8f3d4156dcfde4da76fb3af10025")))

(defun marshal422-test-default-spec (slot)
  (list (cons 'alist slot) (cons 'plist (intern (format ":%s" slot)))))

(marshal-defclass marshal422-person ()
  ((name :initarg :name :type string
         :marshal ((alist . full_name) (plist . :full-name) json))
   (age :initarg :age :type integer
        :marshal ((alist . years) (plist . :years) json))
   (active :initarg :active :marshal-type bool
           :marshal ((alist . enabled) (plist . :enabled) json))
   (note :initarg :note
         :marshal ((alist . annotation) (plist . :annotation) json))))

(marshal-defclass marshal422-defaulted ()
  ((alpha :initarg :alpha :type string)
   (beta :initarg :beta :type integer))
  :marshal-default-spec marshal422-test-default-spec)

(marshal-defclass marshal422-node ()
  ((id :initarg :id :type string :marshal ((plist . :id) json))
   (enabled :initarg :enabled :marshal-type bool
            :marshal ((plist . :enabled) json))
   (children :initarg :children :initform nil
             :marshal-type (list marshal422-node)
             :marshal ((plist . :children) json))))

(marshal-defclass marshal422-dictionary ()
  ((entries :initarg :entries
            :marshal-type (hash string marshal422-node)
            :marshal (json))))

(marshal-defclass marshal422-animal ()
  ((name :initarg :name :type string :marshal (plist)))
  :marshal-class-slot :kind)

(marshal-defclass marshal422-cat (marshal422-animal)
  ((lives :initarg :lives :type integer :marshal (plist))))

(marshal-defclass marshal422-envelope ()
  ((payload :initarg :payload :marshal-type marshal422-animal
            :marshal (plist))))

(defclass marshal422-driver (marshal-driver-plist) ())

(cl-defmethod marshal-postprocess ((driver marshal422-driver) blob)
  (list :wire blob))

(cl-defmethod marshal-preprocess ((driver marshal422-driver) blob)
  (unless (and (listp blob) (plist-member blob :wire))
    (error "Malformed marshal422 wire value: %S" blob))
  (plist-get blob :wire))

(marshal-register-driver 'marshal422-wire 'marshal422-driver)

(marshal-defclass marshal422-packet ()
  ((label :initarg :label :type string
          :marshal ((marshal422-wire . :label)))
   (count :initarg :count :type integer
          :marshal ((marshal422-wire . :count)))))

(defclass marshal422-api-view-base (marshal-base) ())
(defclass marshal422-cache-view-base (marshal-base) ())

(marshal-defclass marshal422-api-view ()
  ((label :initarg :label :type string :marshal ((plist . :api-label)))
   (revision :initarg :revision :type integer
             :marshal ((plist . :api-revision))))
  :marshal-base-cls marshal422-api-view-base)

(marshal-defclass marshal422-cache-view ()
  ((label :initarg :label :type string :marshal ((plist . :cache-name)))
   (revision :initarg :revision :type integer
             :marshal ((plist . :cache-generation))))
  :marshal-base-cls marshal422-cache-view-base)

(defun marshal422-test-sha (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (secure-hash 'sha256 (current-buffer))))

(defun marshal422-test-source-state ()
  (let* ((located (symbol-file 'marshal 'defun))
         (main (and located (file-truename located)))
         (directory (and main (file-name-directory main)))
         (files
          (and directory
               (sort
                (mapcar (lambda (file) (file-relative-name file directory))
                        (seq-filter
                         (lambda (file)
                           (and (string-suffix-p ".el" file)
                                (not (string-suffix-p "-autoloads.el" file))))
                         (directory-files-recursively directory "\\.el\\'")))
                #'string<))))
    (unless (and located main
                 (not (file-symlink-p located))
                 (equal files (mapcar #'car marshal422-test-manifest)))
      (error "Unexpected installed Marshal payload: %S" files))
    (dolist (entry marshal422-test-manifest)
      (let ((file (expand-file-name (car entry) directory)))
        (unless (and (file-regular-p file)
                     (not (file-symlink-p file))
                     (equal (marshal422-test-sha file) (cdr entry)))
          (error "Unexpected installed Marshal source: %S" entry))))
    (list :tree marshal422-test-tree
          :manifest marshal422-test-manifest
          :feature (featurep 'marshal)
          :version "20201223.1853"
          :drivers (mapcar (lambda (entry) (cons (car entry) (cdr entry)))
                           marshal-drivers))))

(defun marshal422-test-condition (thunk)
  (condition-case condition
      (list :returned (funcall thunk))
    (error
     (list :error (car condition)
           :data (copy-tree (cdr condition))
           :message (error-message-string condition)))))

(defun marshal422-test-window-state ()
  (mapcar
   (lambda (window)
     (list window
           (eq window (selected-window))
           (window-buffer window)
           (window-point window)
           (window-start window)
           (window-hscroll window)
           (window-dedicated-p window)
           (window-edges window)))
   (seq-mapcat (lambda (frame) (window-list frame 'nomini)) (frame-list))))

(defun marshal422-test-forbid-external (operation &rest arguments)
  (error "Unexpected Marshal external boundary: %S %S" operation arguments))

(defun marshal422-test-run (body)
  (let* ((buffer-before (current-buffer))
         (buffers-before (buffer-list))
         (processes-before (process-list))
         (timers-before (append timer-list timer-idle-list))
         (frames-before (frame-list))
         (selected-window-before (selected-window))
         (window-before (current-window-configuration))
         (window-state-before (marshal422-test-window-state))
         (source-before (marshal422-test-source-state))
         (drivers-before marshal-drivers)
         (drivers-value-before (copy-tree marshal-drivers))
         result source-after cleanup-errors)
    (unwind-protect
        (progn
          (cl-letf (((symbol-function 'call-process)
                     (lambda (&rest args)
                       (apply #'marshal422-test-forbid-external 'call-process args)))
                    ((symbol-function 'call-process-region)
                     (lambda (&rest args)
                       (apply #'marshal422-test-forbid-external
                              'call-process-region args)))
                    ((symbol-function 'process-file)
                     (lambda (&rest args)
                       (apply #'marshal422-test-forbid-external 'process-file args)))
                    ((symbol-function 'start-process)
                     (lambda (&rest args)
                       (apply #'marshal422-test-forbid-external 'start-process args)))
                    ((symbol-function 'start-file-process)
                     (lambda (&rest args)
                       (apply #'marshal422-test-forbid-external
                              'start-file-process args)))
                    ((symbol-function 'make-process)
                     (lambda (&rest args)
                       (apply #'marshal422-test-forbid-external 'make-process args)))
                    ((symbol-function 'make-network-process)
                     (lambda (&rest args)
                       (apply #'marshal422-test-forbid-external
                              'make-network-process args)))
                    ((symbol-function 'url-retrieve)
                     (lambda (&rest args)
                       (apply #'marshal422-test-forbid-external 'url-retrieve args)))
                    ((symbol-function 'url-retrieve-synchronously)
                     (lambda (&rest args)
                       (apply #'marshal422-test-forbid-external
                              'url-retrieve-synchronously args))))
            (setq result (funcall body)))
          (setq source-after (marshal422-test-source-state))
          (unless (equal source-before source-after)
            (error "Marshal source or driver registry changed")))
      (cl-labels
          ((attempt (label thunk)
             (condition-case condition
                 (funcall thunk)
               (error
                (push (list label (car condition) (copy-tree (cdr condition)))
                      cleanup-errors)))))
        (dolist (process (process-list))
          (unless (memq process processes-before)
            (attempt (list 'process (process-name process))
                     (lambda () (delete-process process)))))
        (dolist (buffer (buffer-list))
          (unless (memq buffer buffers-before)
            (attempt (list 'buffer (buffer-name buffer))
                     (lambda () (kill-buffer buffer)))))
        (dolist (timer (append timer-list timer-idle-list))
          (unless (memq timer timers-before)
            (attempt 'timer (lambda () (cancel-timer timer)))))
        (dolist (frame (frame-list))
          (unless (memq frame frames-before)
            (attempt 'frame (lambda () (delete-frame frame t)))))
        (attempt 'window (lambda () (set-window-configuration window-before)))
        (when (buffer-live-p buffer-before)
          (attempt 'current-buffer (lambda () (set-buffer buffer-before))))))
    (let ((cleanup
           (list :source-unchanged (equal source-before source-after)
                 :drivers-restored
                 (and (eq marshal-drivers drivers-before)
                      (equal marshal-drivers drivers-value-before))
                 :new-buffers
                 (mapcar #'buffer-name
                         (seq-remove (lambda (buffer) (memq buffer buffers-before))
                                     (buffer-list)))
                 :new-processes
                 (length (seq-remove (lambda (process) (memq process processes-before))
                                     (process-list)))
                 :new-timers
                 (length (seq-remove (lambda (timer) (memq timer timers-before))
                                     (append timer-list timer-idle-list)))
                 :new-frames
                 (length (seq-remove (lambda (frame) (memq frame frames-before))
                                     (frame-list)))
                 :buffer-restored (eq (current-buffer) buffer-before)
                 :window-restored
                 (and (eq (selected-window) selected-window-before)
                      (equal (marshal422-test-window-state) window-state-before))
                 :cleanup-errors (nreverse cleanup-errors))))
      (if cleanup-errors
          (error "Marshal cleanup failed: %S" (list result cleanup))
        (list :source source-before :result result :cleanup cleanup)))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(MARSHAL_MELPA_PIN, "marshal.el")
        .expect("prepare exact Marshal source below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn public_alist_and_plist_round_trips_honor_field_specs() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_alist_and_plist_round_trips_honor_field_specs",
        r####"
(marshal422-test-run
 (lambda ()
   (let* ((person (make-instance 'marshal422-person
                                 :name "Café 界" :age 42 :active nil
                                 :note "memo λ"))
          (alist (marshal person 'alist))
          (plist (marshal person 'plist))
          (from-alist (unmarshal 'marshal422-person alist 'alist))
          (from-plist (unmarshal 'marshal422-person plist 'plist))
          (defaulted (make-instance 'marshal422-defaulted
                                    :alpha "mapped" :beta 7)))
     (list :alist alist
           :plist plist
           :alist-object
           (list (oref from-alist name) (oref from-alist age)
                 (oref from-alist active) (oref from-alist note))
           :plist-object
           (list (oref from-plist name) (oref from-plist age)
                 (oref from-plist active) (oref from-plist note))
           :default-alist (marshal defaulted 'alist)
           :default-plist (marshal defaulted 'plist)
           :metadata
           (list (marshal-get-marshal-info 'marshal422-person)
                 (marshal-get-type-info 'marshal422-person))))))
"####,
        expect![[
            r#"OK (:source (:tree "0e52f8ef6216f21d14bf67e9162140491d7f10cb" :manifest (("marshal-pkg.el" . "ad5c070d6b5ee4823b04fc7e46c6d88738e508d49eddaa6e6e1199535ad6ed49") ("marshal.el" . "b1056794b254eaa2ab917534843abfa37b6d8f3d4156dcfde4da76fb3af10025")) :feature t :version "20201223.1853" :drivers ((marshal422-wire . marshal422-driver) (json . marshal-driver-json) (plist . marshal-driver-plist) (alist . marshal-driver-alist))) :result (:alist ((annotation . "memo λ") (enabled) (years . 42) (full_name . "Café 界")) :plist (:full-name "Café 界" :years 42 :enabled nil :annotation "memo λ") :alist-object ("Café 界" 42 nil "memo λ") :plist-object ("Café 界" 42 nil "memo λ") :default-alist ((beta . 7) (alpha . "mapped")) :default-plist (:alpha "mapped" :beta 7) :metadata (((json (note . note) (active . active) (age . age) (name . name)) (plist (note . :annotation) (active . :enabled) (age . :years) (name . :full-name)) (alist (note . annotation) (active . enabled) (age . years) (name . full_name))) ((name . string) (age . integer) (active . bool)))) :cleanup (:source-unchanged t :drivers-restored t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :buffer-restored t :window-restored t :cleanup-errors nil))"#
        ]],
    )
}

fn public_json_round_trip_preserves_recursive_lists_booleans_and_unicode() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_json_round_trip_preserves_recursive_lists_booleans_and_unicode",
        r####"
(marshal422-test-run
 (lambda ()
   (let* ((node (make-instance
                 'marshal422-node :id "root 界" :enabled t
                 :children
                 (list (make-instance 'marshal422-node
                                      :id "café" :enabled nil)
                       (make-instance
                        'marshal422-node :id "branch" :enabled t
                        :children
                        (list (make-instance 'marshal422-node
                                             :id "leaf" :enabled nil))))))
          (json (marshal node 'json))
          (copy (unmarshal 'marshal422-node json 'json))
          (null-object (unmarshal 'marshal422-node "null" 'json)))
     (list :json json
           :roundtrip (marshal copy 'json)
           :root (list (oref copy id) (oref copy enabled))
           :children
           (mapcar (lambda (child)
                     (list (eieio-object-class child)
                           (oref child id)
                           (oref child enabled)
                           (length (oref child children))))
                   (oref copy children))
           :null-json (marshal nil 'json)
           :null-object
           (list (eieio-object-class null-object)
                 (slot-boundp null-object 'id)
                 (slot-boundp null-object 'enabled)
                 (oref null-object children))))))
"####,
        expect![[
            r#"OK (:source (:tree "0e52f8ef6216f21d14bf67e9162140491d7f10cb" :manifest (("marshal-pkg.el" . "ad5c070d6b5ee4823b04fc7e46c6d88738e508d49eddaa6e6e1199535ad6ed49") ("marshal.el" . "b1056794b254eaa2ab917534843abfa37b6d8f3d4156dcfde4da76fb3af10025")) :feature t :version "20201223.1853" :drivers ((marshal422-wire . marshal422-driver) (json . marshal-driver-json) (plist . marshal-driver-plist) (alist . marshal-driver-alist))) :result (:json "{\"children\":[{\"children\":null,\"enabled\":false,\"id\":\"café\"},{\"children\":[{\"children\":null,\"enabled\":false,\"id\":\"leaf\"}],\"enabled\":true,\"id\":\"branch\"}],\"enabled\":true,\"id\":\"root 界\"}" :roundtrip "{\"children\":[{\"children\":null,\"enabled\":false,\"id\":\"café\"},{\"children\":[{\"children\":null,\"enabled\":false,\"id\":\"leaf\"}],\"enabled\":true,\"id\":\"branch\"}],\"enabled\":true,\"id\":\"root 界\"}" :root ("root 界" t) :children ((marshal422-node "café" nil 0) (marshal422-node "branch" t 1)) :null-json "null" :null-object (marshal422-node nil nil nil)) :cleanup (:source-unchanged t :drivers-restored t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :buffer-restored t :window-restored t :cleanup-errors nil))"#
        ]],
    )
}

fn public_typed_hash_round_trip_reconstructs_object_values() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_typed_hash_round_trip_reconstructs_object_values",
        r####"
(marshal422-test-run
 (lambda ()
   (let* ((table (ht-create))
          (_ (ht-set table "ключ café"
                     (make-instance 'marshal422-node
                                    :id "value 界" :enabled nil)))
          (_ (ht-set table "alpha"
                     (make-instance 'marshal422-node
                                    :id "second λ" :enabled t
                                    :children
                                    (list (make-instance 'marshal422-node
                                                         :id "nested" :enabled nil)))))
          (object (make-instance 'marshal422-dictionary :entries table))
          (json (marshal object 'json))
          (copy (unmarshal 'marshal422-dictionary json 'json))
          (copy-table (oref copy entries))
          (values
           (mapcar
            (lambda (key)
              (let ((value (ht-get copy-table key)))
                (list key (eieio-object-class value)
                      (oref value id) (oref value enabled)
                      (mapcar (lambda (child) (oref child id))
                              (oref value children)))))
            (sort (ht-keys copy-table) #'string<))))
     (list :json json
           :roundtrip (marshal copy 'json)
           :hash-table (hash-table-p copy-table)
           :count (hash-table-count copy-table)
           :keys (sort (ht-keys copy-table) #'string<)
           :values values))))
"####,
        expect![[
            r#"OK (:source (:tree "0e52f8ef6216f21d14bf67e9162140491d7f10cb" :manifest (("marshal-pkg.el" . "ad5c070d6b5ee4823b04fc7e46c6d88738e508d49eddaa6e6e1199535ad6ed49") ("marshal.el" . "b1056794b254eaa2ab917534843abfa37b6d8f3d4156dcfde4da76fb3af10025")) :feature t :version "20201223.1853" :drivers ((marshal422-wire . marshal422-driver) (json . marshal-driver-json) (plist . marshal-driver-plist) (alist . marshal-driver-alist))) :result (:json "{\"entries\":{\"alpha\":{\"children\":[{\"children\":null,\"enabled\":false,\"id\":\"nested\"}],\"enabled\":true,\"id\":\"second λ\"},\"ключ café\":{\"children\":null,\"enabled\":false,\"id\":\"value 界\"}}}" :roundtrip "{\"entries\":{\"alpha\":{\"children\":[{\"children\":null,\"enabled\":false,\"id\":\"nested\"}],\"enabled\":true,\"id\":\"second λ\"},\"ключ café\":{\"children\":null,\"enabled\":false,\"id\":\"value 界\"}}}" :hash-table t :count 2 :keys ("alpha" "ключ café") :values (("alpha" marshal422-node "second λ" t ("nested")) ("ключ café" marshal422-node "value 界" nil nil))) :cleanup (:source-unchanged t :drivers-restored t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :buffer-restored t :window-restored t :cleanup-errors nil))"#
        ]],
    )
}

fn public_subclass_discriminator_and_custom_driver_round_trip() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_subclass_discriminator_and_custom_driver_round_trip",
        r####"
(marshal422-test-run
 (lambda ()
   (let* ((envelope
           (make-instance 'marshal422-envelope
                          :payload (make-instance 'marshal422-cat
                                                  :name "Mochi 界" :lives 9)))
          (plist (marshal envelope 'plist))
          (copy (unmarshal 'marshal422-envelope plist 'plist))
          (payload (oref copy payload))
          (packet (make-instance 'marshal422-packet
                                 :label "wire café" :count 3))
          (wire (marshal packet 'marshal422-wire))
          (packet-copy (unmarshal 'marshal422-packet wire 'marshal422-wire)))
     (list :plist plist
           :payload (list (eieio-object-class payload)
                          (oref payload name) (oref payload lives))
           :class-slot (marshal-get-class-slot 'marshal422-animal)
           :wire wire
           :packet (list (eieio-object-class packet-copy)
                         (oref packet-copy label) (oref packet-copy count))
           :driver (eieio-object-class
                    (marshal-get-driver 'marshal422-wire))))))
"####,
        expect![[
            r#"OK (:source (:tree "0e52f8ef6216f21d14bf67e9162140491d7f10cb" :manifest (("marshal-pkg.el" . "ad5c070d6b5ee4823b04fc7e46c6d88738e508d49eddaa6e6e1199535ad6ed49") ("marshal.el" . "b1056794b254eaa2ab917534843abfa37b6d8f3d4156dcfde4da76fb3af10025")) :feature t :version "20201223.1853" :drivers ((marshal422-wire . marshal422-driver) (json . marshal-driver-json) (plist . marshal-driver-plist) (alist . marshal-driver-alist))) :result (:plist (payload (:kind marshal422-cat name "Mochi 界" lives 9)) :payload (marshal422-cat "Mochi 界" 9) :class-slot :kind :wire (:wire (:label "wire café" :count 3)) :packet (marshal422-packet "wire café" 3) :driver marshal422-driver) :cleanup (:source-unchanged t :drivers-restored t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :buffer-restored t :window-restored t :cleanup-errors nil))"#
        ]],
    )
}

fn public_private_base_classes_keep_multiple_views_out_of_global_driver_state() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_private_base_classes_keep_multiple_views_out_of_global_driver_state",
        r####"
(marshal422-test-run
 (lambda ()
   (let* ((registry marshal-drivers)
          (registry-value (copy-tree marshal-drivers))
          (api (make-instance 'marshal422-api-view
                              :label "shared café 界" :revision 11))
          (cache (make-instance 'marshal422-cache-view
                                :label "shared café 界" :revision 11))
          (api-plist (marshal api 'plist))
          (cache-plist (marshal cache 'plist))
          (api-copy (unmarshal 'marshal422-api-view api-plist 'plist))
          (cache-copy (unmarshal 'marshal422-cache-view cache-plist 'plist)))
     (list :api
           (list api-plist
                 (eieio-object-class api-copy)
                 (oref api-copy label) (oref api-copy revision)
                 (object-of-class-p api-copy 'marshal422-api-view-base)
                 (object-of-class-p api-copy 'marshal422-cache-view-base))
           :cache
           (list cache-plist
                 (eieio-object-class cache-copy)
                 (oref cache-copy label) (oref cache-copy revision)
                 (object-of-class-p cache-copy 'marshal422-cache-view-base)
                 (object-of-class-p cache-copy 'marshal422-api-view-base))
           :same-driver-class
           (eq (eieio-object-class (marshal-get-driver 'plist))
               'marshal-driver-plist)
           :registry-unchanged
           (and (eq marshal-drivers registry)
                (equal marshal-drivers registry-value))))))
"####,
        expect![[
            r#"OK (:source (:tree "0e52f8ef6216f21d14bf67e9162140491d7f10cb" :manifest (("marshal-pkg.el" . "ad5c070d6b5ee4823b04fc7e46c6d88738e508d49eddaa6e6e1199535ad6ed49") ("marshal.el" . "b1056794b254eaa2ab917534843abfa37b6d8f3d4156dcfde4da76fb3af10025")) :feature t :version "20201223.1853" :drivers ((marshal422-wire . marshal422-driver) (json . marshal-driver-json) (plist . marshal-driver-plist) (alist . marshal-driver-alist))) :result (:api ((:api-label "shared café 界" :api-revision 11) marshal422-api-view "shared café 界" 11 t nil) :cache ((:cache-name "shared café 界" :cache-generation 11) marshal422-cache-view "shared café 界" 11 t nil) :same-driver-class t :registry-unchanged t) :cleanup (:source-unchanged t :drivers-restored t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :buffer-restored t :window-restored t :cleanup-errors nil))"#
        ]],
    )
}

fn exact_failures_are_atomic_and_public_recovery_succeeds() -> ParityBatchCase {
    ParityBatchCase::value(
        "exact_failures_are_atomic_and_public_recovery_succeeds",
        r####"
(marshal422-test-run
 (lambda ()
   (let* ((alist-driver (marshal-get-driver 'alist))
          (read-before-open
           (marshal422-test-condition
            (lambda () (marshal-read alist-driver 'missing))))
          (write-before-open
           (marshal422-test-condition
            (lambda () (marshal-write alist-driver 'field 1))))
          (closed-state
           (list :input (slot-boundp alist-driver 'input)
                 :output (slot-boundp alist-driver 'output)))
          (_ (marshal-open alist-driver))
          (_ (marshal-write alist-driver 'recovered "same driver 界"))
          (same-driver-output (marshal-close alist-driver))
          (write-state
           (list :input (slot-boundp alist-driver 'input)
                 :output (slot-boundp alist-driver 'output)))
          (_ (slot-makeunbound alist-driver 'output))
          (_ (marshal-open alist-driver '((recovered . "read café"))))
          (same-driver-input (marshal-read alist-driver 'recovered))
          (_ (marshal-close alist-driver))
          (read-state
           (list :input (slot-boundp alist-driver 'input)
                 :output (slot-boundp alist-driver 'output)))
          (bad-json
           (marshal422-test-condition
            (lambda ()
              (unmarshal 'marshal422-person
                         "{\"name\": [}" 'json))))
          (bad-wire
           (marshal422-test-condition
            (lambda ()
              (unmarshal 'marshal422-packet '(:wrong t)
                         'marshal422-wire))))
          (recovered-json
           (unmarshal 'marshal422-person
                      "{\"name\":\"recovered 界\",\"age\":8,\"active\":true}"
                      'json))
          (recovered-wire
           (unmarshal 'marshal422-packet
                      '(:wire (:label "ok café" :count 2))
                      'marshal422-wire)))
     (list :read-before-open read-before-open
           :write-before-open write-before-open
           :closed-state closed-state
           :same-driver-output same-driver-output
           :write-state write-state
           :same-driver-input same-driver-input
           :read-state read-state
           :bad-json bad-json
           :bad-wire bad-wire
           :recovered-json
           (list (oref recovered-json name) (oref recovered-json age)
                 (oref recovered-json active)
                 (oref recovered-json note))
           :recovered-wire
           (list (oref recovered-wire label) (oref recovered-wire count))))))
"####,
        expect![[
            r#"OK (:source (:tree "0e52f8ef6216f21d14bf67e9162140491d7f10cb" :manifest (("marshal-pkg.el" . "ad5c070d6b5ee4823b04fc7e46c6d88738e508d49eddaa6e6e1199535ad6ed49") ("marshal.el" . "b1056794b254eaa2ab917534843abfa37b6d8f3d4156dcfde4da76fb3af10025")) :feature t :version "20201223.1853" :drivers ((marshal422-wire . marshal422-driver) (json . marshal-driver-json) (plist . marshal-driver-plist) (alist . marshal-driver-alist))) :result (:read-before-open (:error error :data ("Driver has not been opened in read mode") :message "Driver has not been opened in read mode") :write-before-open (:error error :data ("Driver has not been opened in write mode") :message "Driver has not been opened in write mode") :closed-state (:input nil :output nil) :same-driver-output ((recovered . "same driver 界")) :write-state (:input nil :output t) :same-driver-input "read café" :read-state (:input t :output nil) :bad-json (:error json-readtable-error :data (125) :message "JSON readtable error: 125") :bad-wire (:error error :data ("Malformed marshal422 wire value: (:wrong t)") :message "Malformed marshal422 wire value: (:wrong t)") :recovered-json ("recovered 界" 8 t nil) :recovered-wire ("ok café" 2)) :cleanup (:source-unchanged t :drivers-restored t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :buffer-restored t :window-restored t :cleanup-errors nil))"#
        ]],
    )
}

#[test]
fn marshal_package_batch() {
    let cases: Vec<ParityBatchCase> = vec![
        public_alist_and_plist_round_trips_honor_field_specs(),
        public_json_round_trip_preserves_recursive_lists_booleans_and_unicode(),
        public_typed_hash_round_trip_reconstructs_object_values(),
        public_subclass_discriminator_and_custom_driver_round_trip(),
        public_private_base_classes_keep_multiple_views_out_of_global_driver_state(),
        exact_failures_are_atomic_and_public_recovery_succeeds(),
    ];
    assert_oracle_batch_cases(oracle(), "marshal-rank422", "marshal_parity", &cases);
}
