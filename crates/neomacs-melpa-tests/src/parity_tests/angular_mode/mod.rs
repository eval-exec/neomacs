use std::time::Duration;

use crate::{ANGULAR_MODE_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ANGULAR_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(120);

/// angular-mode is two derived major modes and nothing else: `angular-mode'
/// adds AngularJS keywords to `javascript-mode', and `angular-html-mode' adds
/// directive and interpolation patterns to `html-mode'.  All of its value is
/// therefore what a user sees on the screen, so these workflows write real
/// `.js' and `.html' files into the per-case sandbox, turn the real mode on,
/// run `font-lock-ensure', and read faces back at real positions.
///
/// There is no external boundary here and nothing is stood in for.
///
/// Two of the workflows exist because reading the source is not enough to know
/// what this package actually paints.  Its keyword lists go through
/// `regexp-opt' with no word boundaries, so they match inside longer
/// identifiers; and `angular-html-mode' appends its patterns *after* the sgml
/// rules, so whether they take effect depends on whether html-mode has already
/// claimed the region.  Both are asserted against a fixture built to contain
/// one case of each outcome, and both are compared against the parent mode on
/// the same text, so a rule that silently never fires cannot look like a rule
/// that works.
const ANGULAR_MODE_TEST_PRELUDE: &str = r##";;; prelude -*- lexical-binding: t; -*-
(require 'cl-lib)

(defun ang-test-path (name)
  (expand-file-name name (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defun ang-test-copy (value)
  (if (stringp value) (copy-sequence value) value))

(defun ang-test-write-file (path text)
  (make-directory (file-name-directory path) t)
  (with-temp-buffer
    (insert text)
    (write-region (point-min) (point-max) path nil 'silent))
  path)

(defconst ang-test-controller-js "\
angular.module('inventory', ['ngRoute'])
  .controller('WidgetCtrl', function ($scope, $http, $timeout) {
    $scope.widgets = [];
    $scope.$watch('widgets', function (next, prev) {
      angular.forEach(next, function (widget) {
        $scope.$broadcast('widget:changed', widget);
      });
    });
    $http.get('/api/widgets').then(function (response) {
      $scope.widgets = angular.copy(response.data);
      $scope.$apply();
    });
  })
  .directive('widgetCard', function () {
    return {
      scope: { widget: '=' },
      templateUrl: 'widget-card.html',
      transclude: true,
      controllerAs: 'card',
      link: function (scope, element) { element.addClass('card'); }
    };
  });

describe('WidgetCtrl', function () {
  beforeEach(module('inventory'));
  it('starts empty', function () {
    expect($scope.widgets.length).toBe(0);
  });
});
")

(defconst ang-test-template-html "\
<!DOCTYPE html>
<html ng-app=\"inventory\">
  <body ng-controller=\"WidgetCtrl as ctrl\">
    <h1>{{ ctrl.title }}</h1>
    <ul>
      <li ng-repeat=\"widget in widgets\" ng-class=\"{sold: widget.sold}\">
        <span ng-bind=\"widget.name\"></span>
        <b>{{ widget.price | currency }}</b>
        <button ng-click=\"buy(widget)\" ng-disabled=\"widget.sold\">Buy</button>
      </li>
    </ul>
    <p ng-hide=\"widgets.length\">Nothing in stock.</p>
    <div>{{ widgets.length }} in stock</div>
  </body>
</html>
")

(defconst ang-test-lookalikes-js "\
var $idle = true;
var myangular = require('not-angular');
myangular.module('fake');
var report = { controllerAsText: 'x', linkText: 'y' };
inventory.controllers.push('WidgetCtrl');
element.forEachChild(function (child) { child.$idleTimeout(); });
describeTheWidget();
")

(defun ang-test-visit (name text mode)
  (let ((buffer (find-file-noselect (ang-test-write-file (ang-test-path name) text))))
    (set-window-buffer (selected-window) buffer)
    (set-buffer buffer)
    (funcall mode)
    (font-lock-ensure)
    buffer))

(defun ang-test-face-of (token &optional occurrence)
  "The face at the start of the OCCURRENCE-th TOKEN, with its bounds."
  (save-excursion
    (goto-char (point-min))
    (when (search-forward token nil t (or occurrence 1))
      (let* ((start (match-beginning 0))
             (face (get-text-property start 'face)))
        (list :token (copy-sequence token)
              :face face
              :column (save-excursion (goto-char start)
                                      (- start (line-beginning-position)))
              :line (line-number-at-pos start))))))

(defun ang-test-faces-on-line (line)
  "Every face run on LINE, as (FACE TEXT)."
  (save-excursion
    (goto-char (point-min))
    (forward-line (1- line))
    (let ((end (line-end-position)) (position (point)) runs)
      (while (< position end)
        (let ((next (next-single-property-change position 'face nil end)))
          (push (list (get-text-property position 'face)
                      (buffer-substring-no-properties position next))
                runs)
          (setq position next)))
      (nreverse runs))))

(defun ang-test-tokens-with-face (face)
  "Every distinct string carrying FACE, in order of first appearance."
  (let ((position (point-min)) seen)
    (while (< position (point-max))
      (let ((next (next-single-property-change position 'face nil (point-max))))
        (when (equal (get-text-property position 'face) face)
          (let ((text (buffer-substring-no-properties position next)))
            (unless (member text seen) (push text seen))))
        (setq position next)))
    (sort seen #'string<)))
"##;

fn angular_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ANGULAR_MODE_MELPA_PIN, "angular-mode.el")
        .expect("prepare pinned angular-mode source below ./tmp")
        .with_prelude(ANGULAR_MODE_TEST_PRELUDE)
        .with_timeout(ANGULAR_MODE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed angular-mode parity test")
        .into()
}

/// Multi-probe batch for `assert_angular_mode_parity` cases (2a).
pub(crate) fn assert_angular_mode_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(angular_mode_oracle(), &name, "angular_mode_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn angular_mode_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_angular_mode_batch(&cases);
}

// END generated package batch tests
