use expect_test::expect;

use super::ParityBatchCase;

/// What enabling an airline theme takes away from the stock faces it reuses.
///
/// The package does not only add `airline-*' faces: it re-specifies
/// `mode-line', `mode-line-inactive', `mode-line-buffer-id',
/// `minibuffer-prompt' and the three `tab-bar' faces.  A theme spec replaces
/// the standard definition rather than merging with it, and these specs set
/// little more than a foreground and a background, so whatever else the stock
/// face had is dropped for as long as the theme is enabled.
///
/// `face-default-spec' is recorded beside every loss, because it is the only
/// thing that tells the two kinds apart, and both kinds are present here:
///
///   * `mode-line-buffer-id' is `((t (:weight bold)))' -- an unconditional
///     clause, so its bold is in force for every user on every display.  It
///     shows up under `:changed' rather than `:losses', because with the theme
///     enabled the weight resolves to `normal' from the `default' face instead
///     of vanishing: the buffer name stops being bold while the attribute
///     stays technically specified, which a disappearance-only measure misses
///     entirely;
///   * `mode-line-inactive', `tab-bar-tab' and `tab-bar-tab-inactive' each
///     carry a `default' clause whose `:inherit' likewise applies everywhere;
///   * `mode-line', `minibuffer-prompt' and `tab-bar' hold their attributes on
///     colour-conditional clauses that a frame reporting no colours never
///     matched, so anything "lost" there was never in force to begin with.
///
/// Reporting a count alone, or the losses without their specs, would run those
/// together and read as far more alarming than the truth.  Restoration on
/// disable is asserted too, so a loss is known to be temporary.
fn enabling_a_theme_drops_stock_attributes_from_the_mode_line_faces_it_reuses() -> ParityBatchCase {
    ParityBatchCase::value(
        "enabling_a_theme_drops_stock_attributes_from_the_mode_line_faces_it_reuses",
        r##"(let* ((stock '(mode-line mode-line-inactive mode-line-buffer-id
                    minibuffer-prompt tab-bar tab-bar-tab tab-bar-tab-inactive))
       (before (airline-test-capture stock))
       (during nil)
       (restored nil))
  (airline-test-with-theme
   'airline-doom-one
   (lambda () (setq during (airline-test-capture stock))))
  (setq restored (airline-test-capture stock))
  (list :all-exist-before-the-theme
        (mapcar (lambda (face) (and (facep face) t)) stock)
        :losses (airline-test-losses before during)
        :changed (airline-test-changes before during)
        :restored-on-disable (equal before restored)))"##,
        expect![[
            r##"OK (:all-exist-before-the-theme (t t t t t t t) :losses ((mode-line (:inverse-video) ((((class color grayscale) (min-colors 88) (background light)) :box (:line-width -1 :style released-button) :background "grey75" :foreground "black") (((class color grayscale) (min-colors 88) (background dark)) :box (:line-width -1 :style released-button) :background "grey20" :foreground "white") (t :inverse-video t))) (mode-line-inactive (:inverse-video :inherit) ((default :inherit mode-line) (((class color grayscale) (min-colors 88) (background light)) :weight light :box (:line-width -1 :color "grey75" :style nil) :foreground "grey20" :background "grey90") (((class color grayscale) (min-colors 88) (background dark)) :weight light :box (:line-width -1 :color "grey40" :style nil) :foreground "grey80" :background "grey30"))) (tab-bar-tab (:inherit) ((default :inherit tab-bar) (((class color) (min-colors 88) (background light)) :box (:line-width 1 :style released-button)) (((class color) (min-colors 88) (background dark)) :box (:line-width 1 :style released-button) :background "grey40" :foreground "white") (t :inverse-video nil))) (tab-bar-tab-inactive (:inverse-video :inherit) ((default :inherit tab-bar-tab) (((class color) (min-colors 88) (background light)) :background "grey75") (((class color) (min-colors 88) (background dark)) :background "grey20") (t :inverse-video t)))) :changed ((mode-line-buffer-id :weight bold normal) (minibuffer-prompt :foreground "cyan" "#1B2229") (tab-bar :background "grey" "#21242b") (tab-bar-tab :background "grey" "#21242b") (tab-bar-tab-inactive :background "grey" "#23272e")) :restored-on-disable t)"##
        ]],
    )
    .fresh_process()
}

fn the_tab_bar_faces_take_their_colours_from_each_themes_own_palette() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_tab_bar_faces_take_their_colours_from_each_themes_own_palette",
        r##"(mapcar
 (lambda (theme)
   (let (observed)
     (airline-test-with-theme
      theme
      (lambda ()
        (setq observed
              (mapcar (lambda (face)
                        (list face
                              (face-attribute face :foreground nil 'default)
                              (face-attribute face :background nil 'default)))
                      '(tab-bar tab-bar-tab tab-bar-tab-inactive
                        airline-normal-center airline-normal-inner)))))
     (list theme observed)))
 '(airline-doom-one airline-light))"##,
        expect![[
            r##"OK ((airline-doom-one ((tab-bar "#bbc2cf" "#21242b") (tab-bar-tab "#bbc2cf" "#21242b") (tab-bar-tab-inactive "#5B6268" "#23272e") (airline-normal-center "#bbc2cf" "#21242b") (airline-normal-inner "#bbc2cf" "#21242b"))) (airline-light ((tab-bar "#005fff" "#afffff") (tab-bar-tab "#000087" "#00dfff") (tab-bar-tab-inactive "#666666" "#b2b2b2") (airline-normal-center "#005fff" "#afffff") (airline-normal-inner "#000087" "#00dfff"))))"##
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        enabling_a_theme_drops_stock_attributes_from_the_mode_line_faces_it_reuses(),
        the_tab_bar_faces_take_their_colours_from_each_themes_own_palette(),
    ]
}
