//! Behavioural parity for the pinned `etaf-ncm` NetEase Cloud Music player.
//!
//! The player's backend has a documented injection point: `etaf-ncm-create`
//! accepts any `(REQUEST CALLBACK)` client, and the package's own acceptance
//! suite drives it with a scripted function that answers in backend-shaped
//! fixtures.  These probes use the same seam, so no scenario needs the
//! network, Node, mpv, a login or a graphical frame.
//!
//! Each case drives a documented workflow -- a session, a library refresh,
//! selection, search, transport commands, likes, panes, LRC parsing, layout
//! and palette resolution, and a failing backend -- and asserts concrete state
//! in both editors.  The batch harness requires GNU Emacs and Neomacs to agree
//! exactly; there is no pinned Neomacs outcome.

use std::time::Duration;

use expect_test::expect;

use crate::CachedMelpaOracle;

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

/// Scripted backend, fixtures and state readers shared by every probe.
const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'etaf)
(require 'etaf-ncm)

(defvar neomacs-ncm-requests nil
  "Requests received by the scripted client, newest first.")
(defvar neomacs-ncm-fail nil
  "When non-nil, every scripted request fails with this message.")

(defconst neomacs-ncm-daily
  '(:count 3
    :songs ((:id 1 :name "曲目 1" :artist "歌手 1" :album "专辑 0"
             :ms 180000 :vip nil :plays 1000)
            (:id 2 :name "曲目 2" :artist "歌手 2" :album "专辑 1"
             :ms 181000 :vip t :plays 1001)
            (:id 3 :name "曲目 3" :artist "歌手 3" :album "专辑 2"
             :ms 182000 :vip nil :plays 1002))))

(defconst neomacs-ncm-status
  '(:ok t :conclusion "正在播放：曲目 1 - 歌手 1"
    :title "曲目 1" :artist "歌手 1" :song_id "1" :state "playing"
    :queue_pos "2/3" :elapsed "42" :duration "180" :volume "55"
    :repeat "false" :shuffle "false" :source "mpv"
    :as_of "2026-01-01T00:00:00+0800"))

(defconst neomacs-ncm-queue
  '(:ok t :conclusion "队列 3 首（当前第 2 首）" :count "3" :pos "2"
    :source "mpv" :as_of "2026-01-01T00:00:00+0800"
    :songs ((:id 1 :name "曲目 1" :artist "歌手 1" :album "专辑 1"
             :ms 180000 :vip nil :level "exhigh")
            (:id 2 :name "曲目 2" :artist "歌手 2" :album "专辑 2"
             :ms 181000 :vip t :level "exhigh")
            (:id 3 :name "曲目 3" :artist "歌手 3" :album "专辑 3"
             :ms 182000 :vip nil :level "exhigh"))))

(defconst neomacs-ncm-lyric
  '(:id 1 :lyric "[00:00.50] 第一行歌词\n[00:05.00] 第二行歌词\n[00:10.25] 第三行歌词\n[00:15.00] \n[00:20.00] 第四行歌词\n"))

(defconst neomacs-ncm-session-fixture
  '(:loggedIn t :uid 0 :nick "听众" :vip 0))

(defun neomacs-ncm-reply (request)
  "Return the backend data for REQUEST, shaped like the real JSON."
  (pcase (plist-get request :op)
    ((or :browse :search)
     (pcase (plist-get request :resource)
       (:playlists
        (list :count 1
              :playlists (list (list :id 9 :name "歌单" :creator "听众"
                                     :count 1 :sub nil :special nil))))
       (_ neomacs-ncm-daily)))
    (:lyric neomacs-ncm-lyric)
    (:session neomacs-ncm-session-fixture)
    (:status neomacs-ncm-status)
    (:queue neomacs-ncm-queue)
    (:liked-ids '(:count 2 :total 2 :ids (1 3)))
    (:downloads '(:count 0 :total 0 :downloads nil))
    (_ (list :ok t))))

(defun neomacs-ncm-client (request callback)
  "Answer REQUEST on CALLBACK from fixtures, recording it first."
  (push request neomacs-ncm-requests)
  (funcall callback
           (if neomacs-ncm-fail
               (list :error neomacs-ncm-fail :hint "scripted")
             (list :ok (neomacs-ncm-reply request)))))

(defun neomacs-ncm-fresh ()
  "Reset the scripted client's state."
  (setq neomacs-ncm-requests nil
        neomacs-ncm-fail nil))

(defun neomacs-ncm-session ()
  "Return a session driven by the scripted client."
  (etaf-ncm-create :client #'neomacs-ncm-client))

(defun neomacs-ncm-rows (app)
  "Return APP's library rows compactly."
  (mapcar (lambda (row)
            (list (plist-get row :kind) (plist-get row :id)
                  (plist-get row :name) (plist-get row :subtitle)))
          (plist-get (etaf-ncm-state app) :items)))

(defun neomacs-ncm-status-facts (app)
  "Return APP's playback state compactly."
  (let ((status (plist-get (etaf-ncm-state app) :status)))
    (list (plist-get status :title) (plist-get status :song-id)
          (plist-get status :state) (plist-get status :queue-pos)
          (plist-get status :volume) (plist-get status :liked)
          (plist-get status :repeat) (plist-get status :shuffle))))

(defun neomacs-ncm-facts (app)
  "Return APP's session state compactly."
  (let ((state (etaf-ncm-state app)))
    (list :section (plist-get state :section)
          :mode (plist-get state :mode)
          :layout (plist-get state :layout)
          :aside (plist-get state :aside)
          :pane (plist-get state :pane)
          :rows (neomacs-ncm-rows app)
          :rows-total (plist-get state :rows-total)
          :list-total (plist-get state :list-total)
          :selection (plist-get state :selection)
          :account (plist-get state :account)
          :status (neomacs-ncm-status-facts app)
          :queue-count (plist-get (plist-get state :queue) :count)
          :lyric-rows (plist-get state :lyric-rows)
          :error (plist-get state :error)
          :message (plist-get state :message))))

(defun neomacs-ncm-ops ()
  "Return the recorded requests' operations and arguments, oldest first."
  (mapcar (lambda (request)
            (list (plist-get request :op)
                  (plist-get request :value)
                  (plist-get request :on)
                  (plist-get request :id)
                  (plist-get request :index)
                  (plist-get request :keyword)
                  (plist-get request :scope)
                  (plist-get request :target)))
          (nreverse neomacs-ncm-requests)))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(("etaf-ncm", "0.2.3"), "etaf-ncm.el")
        .expect("prepare pinned etaf-ncm source below ./tmp")
        .with_timeout(Duration::from_secs(240))
        .with_prelude(PRELUDE)
}

#[test]
fn etaf_ncm_behavior_batch() {
    let cases = vec![
        // A session owns no buffer and no timer, starts on the daily library
        // with the lyric aside in the list pane, and sends no request.
        ParityBatchCase::value(
            "session_headless",
            r####"
(progn
  (neomacs-ncm-fresh)
  (let ((app (neomacs-ncm-session)))
    (list (neomacs-ncm-facts app) neomacs-ncm-requests)))
"####,
            expect![
                "OK ((:section :daily :mode browse :layout wide :aside lyrics :pane list :rows nil :rows-total 0 :list-total 0 :selection 0 :account nil :status (nil nil nil nil 0 nil nil nil) :queue-count nil :lyric-rows 17 :error nil :message nil) nil)"
            ],
        ),
        // Refreshing loads the library rows, the account, the playback truth
        // and the queue through the scripted backend.
        ParityBatchCase::value(
            "refresh_library",
            r####"
(progn
  (neomacs-ncm-fresh)
  (let ((app (neomacs-ncm-session)))
    (etaf-ncm-refresh app)
    (list (neomacs-ncm-facts app) (neomacs-ncm-ops))))
"####,
            expect![[
                r#"OK ((:section :daily :mode browse :layout wide :aside lyrics :pane list :rows ((:song 1 "曲目 1" "歌手 1") (:song 2 "曲目 2" "歌手 2") (:song 3 "曲目 3" "歌手 3")) :rows-total 3 :list-total 3 :selection 0 :account (:nick "听众" :uid 0 :vip 0 :logged-in t) :status ("曲目 1" 1 playing 2 55 t nil nil) :queue-count 3 :lyric-rows 17 :error nil :message "每日推荐 · 3 项") ((:browse nil nil nil nil nil nil nil) (:status nil nil nil nil nil nil nil) (:lyric nil nil 1 nil nil nil nil) (:liked-ids nil nil nil nil nil nil nil) (:downloads nil nil nil nil nil nil nil) (:queue nil nil nil nil nil nil nil) (:session nil nil nil nil nil nil nil)))"#
            ]],
        ),
        // Loading a library resets the selection to its first row, and moving
        // the selection stops at both ends of the loaded list.
        ParityBatchCase::value(
            "selection_clamps",
            r####"
(progn
  (neomacs-ncm-fresh)
  (let ((app (neomacs-ncm-session)))
    (etaf-ncm-refresh app)
    (let ((initial (plist-get (etaf-ncm-state app) :selection)))
      (etaf-ncm-move-selection app 10)
      (let ((bottom (plist-get (etaf-ncm-state app) :selection)))
        (etaf-ncm-move-selection app -10)
        (list initial bottom
              (plist-get (etaf-ncm-state app) :selection))))))
"####,
            expect!["OK (0 2 0)"],
        ),
        // Search trims its keyword, records it with the scope, and rejects a
        // blank keyword without querying the backend.
        ParityBatchCase::value(
            "search_trims_and_rejects_blank",
            r####"
(progn
  (neomacs-ncm-fresh)
  (let ((app (neomacs-ncm-session)))
    (etaf-ncm-search app "  陈粒 " 'song)
    (let ((state (etaf-ncm-state app)))
      (list (list (plist-get state :mode)
                  (plist-get state :query)
                  (plist-get state :section))
            (neomacs-ncm-ops)
            (condition-case err
                (etaf-ncm-search app "   " 'song)
              (etaf-ncm-error (list :signal (car err) (cadr err))))))))
"####,
            expect![[
                r#"OK ((search "陈粒" :search) ((:search nil nil nil nil "陈粒" song nil)) (:signal etaf-ncm-error "搜索关键词不能为空"))"#
            ]],
        ),
        // Transport and library commands reach the backend with their
        // documented operations and arguments.
        ParityBatchCase::value(
            "transport_commands",
            r####"
(progn
  (neomacs-ncm-fresh)
  (let ((app (neomacs-ncm-session)))
    (etaf-ncm-refresh app)
    (neomacs-ncm-fresh)
    (let ((rows (plist-get (etaf-ncm-state app) :items)))
      (etaf-ncm-toggle-playback app)
      (etaf-ncm-set-volume app 42)
      (etaf-ncm-seek app -10)
      (etaf-ncm-set-shuffle app t)
      (etaf-ncm-set-repeat app nil)
      (etaf-ncm-next-track app)
      (etaf-ncm-add-to-queue app (car rows))
      (etaf-ncm-play app (cadr rows))
      (list (neomacs-ncm-ops)
            (neomacs-ncm-status-facts app)))))
"####,
            expect![[
                r#"OK (((:toggle nil nil nil nil nil nil nil) (:status nil nil nil nil nil nil nil) (:volume 42 nil nil nil nil nil nil) (:status nil nil nil nil nil nil nil) (:seek nil nil nil nil nil nil nil) (:status nil nil nil nil nil nil nil) (:shuffle nil t nil nil nil nil nil) (:status nil nil nil nil nil nil nil) (:repeat nil nil nil nil nil nil nil) (:status nil nil nil nil nil nil nil) (:next nil nil nil nil nil nil nil) (:status nil nil nil nil nil nil nil) (:queue nil nil nil nil nil nil nil) (:add nil nil 1 nil nil nil nil) (:status nil nil nil nil nil nil nil) (:queue nil nil nil nil nil nil nil) (:play nil nil nil nil nil nil (:kind :daily)) (:status nil nil nil nil nil nil nil) (:queue nil nil nil nil nil nil nil) (:queue nil nil nil nil nil nil nil)) ("曲目 1" 1 playing 2 55 t nil nil))"#
            ]],
        ),
        // The liked-id table decides the heart, and liking sends the request
        // and keeps the session table in step.
        ParityBatchCase::value(
            "like_heart",
            r####"
(progn
  (neomacs-ncm-fresh)
  (let ((app (neomacs-ncm-session)))
    (etaf-ncm-refresh app)
    (neomacs-ncm-fresh)
    (let ((liked-before (nth 5 (neomacs-ncm-status-facts app))))
      (etaf-ncm-like app)
      (let ((liked-after (nth 5 (neomacs-ncm-status-facts app))))
        (etaf-ncm-like app)
        (list liked-before liked-after
              (nth 5 (neomacs-ncm-status-facts app))
              (neomacs-ncm-ops))))))
"####,
            expect![
                "OK (t t t ((:like nil nil nil nil nil nil nil) (:status nil nil nil nil nil nil nil) (:like nil nil nil nil nil nil nil) (:status nil nil nil nil nil nil nil)))"
            ],
        ),
        // The aside toggles between lyrics and the queue, and an unknown pane
        // signals without changing the previous pane.
        ParityBatchCase::value(
            "panes",
            r####"
(progn
  (neomacs-ncm-fresh)
  (let ((app (neomacs-ncm-session)))
    (let ((initial (plist-get (etaf-ncm-state app) :aside)))
      (etaf-ncm-toggle-aside app)
      (let ((toggled (plist-get (etaf-ncm-state app) :aside)))
        (etaf-ncm-show-pane app 'list)
        (list initial toggled
              (list (plist-get (etaf-ncm-state app) :aside)
                    (plist-get (etaf-ncm-state app) :pane))
              (condition-case err
                  (etaf-ncm-show-pane app 'bogus)
                (etaf-ncm-error (list (car err) (cadr err))))
              (plist-get (etaf-ncm-state app) :pane))))))
"####,
            expect![[
                r#"OK (lyrics queue (queue list) (etaf-ncm-error "Unknown pane: bogus") list)"#
            ]],
        ),
        // LRC parsing keeps tagged lines in time order, splits repeated
        // timestamps, and drops metadata and untagged lines.
        ParityBatchCase::value(
            "lrc_parse",
            r####"
(list
 (etaf-ncm-parse-lyric
  "[00:10.25] Later\n[00:00.50] First\n[00:00.50][00:02.00] Twice\nmetadata: x\n[00:15.00]\n")
 (etaf-ncm-parse-lyric "[00:01.00] name_with_underscores" t))
"####,
            expect![[
                r#"OK (((0.5 . "First") (0.5 . "Twice") (2.0 . "Twice") (10.25 . "Later") (15.0 . "")) ((1.0 . "name with underscores")))"#
            ]],
        ),
        // The layout families follow the width and height thresholds.
        ParityBatchCase::value(
            "layout_families",
            r####"
(list
 (mapcar (lambda (columns) (etaf-ncm-layout-for columns 40))
         '(120 80 40 20 10))
 (mapcar (lambda (rows) (etaf-ncm-layout-for 40 rows))
         '(40 10 5))
 (list (etaf-ncm-layout-small-p 'narrow 40 10)
       (etaf-ncm-layout-small-p 'wide 40 10)
       (etaf-ncm-layout-compact-p 'narrow)
       (etaf-ncm-layout-compact-p 'wide)))
"####,
            expect!["OK ((medium medium card tile tile) (card tile tile) (t nil t nil))"],
        ),
        // Palette tokens resolve per mode and an unknown color signals the
        // package error instead of a silent fallback.
        ParityBatchCase::value(
            "theme_tokens",
            r####"
(list (etaf-ncm-color :bg 'light)
      (etaf-ncm-color :bg 'dark)
      (etaf-ncm-theme-defaults 'light)
      (etaf-ncm-theme-defaults 'dark)
      (condition-case err
          (etaf-ncm-color :not-a-color 'light)
        (etaf-ncm-error (list (car err) (cadr err)))))
"####,
            expect![[
                r##"OK ("#FFFFFF" "#191a1c" (:ui-fg "#1B1F24" :ui-bg "#FFFFFF" :ui-border "#E1E4E9" :ui-muted-fg "#5C6470" :ui-disabled-fg "#5C6470" :ui-disabled-bg "#F6F7F9" :ui-disabled-border "#E1E4E9" :ui-accent "#D8232A" :ui-accent-fg "#FFFFFF" :ui-accent-soft-bg "#FBE9EA" :ui-button-primary-fg "#FFFFFF" :ui-button-primary-bg "#D8232A" :ui-button-secondary-fg "#D8232A" :ui-button-secondary-bg "#FBE9EA" :ui-button-ghost-fg "#5C6470" :ui-success-fg "#2E7D4F" :ui-success-bg "#E3F3E8" :ui-success-border "#A9D8BB" :ui-warning-fg "#B07A16" :ui-warning-bg "#FBF0DA" :ui-warning-border "#E0C48A" :ui-danger-fg "#C0392B" :ui-danger-bg "#FBE4E2" :ui-danger-border "#E8B4AE" :ui-info-fg "#2C6FA8" :ui-info-bg "#E4F0FA" :ui-info-border "#A8CBE6" :ui-panel-fg "#1B1F24" :ui-panel-bg "#F6F7F9" :ui-panel-border "#E1E4E9" :ui-divider-bg "#E1E4E9" :ui-progress-track "#E4E7EB" :ui-range-track "#E4E7EB" :ui-range-fill "#D8232A" :ui-table-border "#E1E4E9" :ui-table-selected-fg "#D8232A" :ui-table-selected-bg "#FBE9EA" :ui-shell-bg "#F6F7F9" :ui-shell-fg "#1B1F24" :ui-shell-muted-fg "#5C6470" :ui-shell-item-fg "#5C6470" :ui-shell-item-minimized-fg "#5C6470" :ui-window-bg "#FFFFFF" :ui-window-fg "#1B1F24" :ui-window-border "#E1E4E9" :ui-window-title-bg "#F6F7F9" :ui-window-title-fg "#5C6470" :ui-window-title-active-fg "#1B1F24" :ui-picker-item-bg "#F6F7F9" :ui-picker-item-fg "#1B1F24" :ui-picker-item-muted-fg "#5C6470" :ui-pagination-muted-fg "#5C6470" :ui-data-grid-error-fg "#C0392B" :ui-checkbox-enabled-fg "#1B1F24" :ui-checkbox-disabled-fg "#5C6470") (:ui-fg "#E9ECF1" :ui-bg "#191a1c" :ui-border "#303338" :ui-muted-fg "#98A0AC" :ui-disabled-fg "#98A0AC" :ui-disabled-bg "#202225" :ui-disabled-border "#303338" :ui-accent "#EC4141" :ui-accent-fg "#FFFFFF" :ui-accent-soft-bg "#2A1619" :ui-button-primary-fg "#FFFFFF" :ui-button-primary-bg "#EC4141" :ui-button-secondary-fg "#EC4141" :ui-button-secondary-bg "#2A1619" :ui-button-ghost-fg "#98A0AC" :ui-success-fg "#4FB477" :ui-success-bg "#17301F" :ui-success-border "#2E5C3F" :ui-warning-fg "#E8B45C" :ui-warning-bg "#33280F" :ui-warning-border "#6B5423" :ui-danger-fg "#FF7A7A" :ui-danger-bg "#3A1D22" :ui-danger-border "#7A2A2C" :ui-info-fg "#6FB4E8" :ui-info-bg "#14232F" :ui-info-border "#2C4E66" :ui-panel-fg "#E9ECF1" :ui-panel-bg "#202225" :ui-panel-border "#303338" :ui-divider-bg "#303338" :ui-progress-track "#2b2e33" :ui-range-track "#2b2e33" :ui-range-fill "#EC4141" :ui-table-border "#303338" :ui-table-selected-fg "#EC4141" :ui-table-selected-bg "#2A1619" :ui-shell-bg "#202225" :ui-shell-fg "#E9ECF1" :ui-shell-muted-fg "#98A0AC" :ui-shell-item-fg "#98A0AC" :ui-shell-item-minimized-fg "#98A0AC" :ui-window-bg "#191a1c" :ui-window-fg "#E9ECF1" :ui-window-border "#303338" :ui-window-title-bg "#202225" :ui-window-title-fg "#98A0AC" :ui-window-title-active-fg "#E9ECF1" :ui-picker-item-bg "#202225" :ui-picker-item-fg "#E9ECF1" :ui-picker-item-muted-fg "#98A0AC" :ui-pagination-muted-fg "#98A0AC" :ui-data-grid-error-fg "#FF7A7A" :ui-checkbox-enabled-fg "#E9ECF1" :ui-checkbox-disabled-fg "#98A0AC") (etaf-ncm-error "Unknown color: :not-a-color"))"##
            ]],
        ),
        // Locating a library loads its rows; an unknown section signals and
        // leaves the current library untouched.
        ParityBatchCase::value(
            "locate_sections",
            r####"
(progn
  (neomacs-ncm-fresh)
  (let ((app (neomacs-ncm-session)))
    (etaf-ncm-refresh app)
    (let ((daily (plist-get (etaf-ncm-state app) :section)))
      (etaf-ncm-locate app :recent)
      (let ((recent (list (plist-get (etaf-ncm-state app) :section)
                          (plist-get (etaf-ncm-state app) :rows-total))))
        (list daily recent
              (condition-case err
                  (etaf-ncm-locate app 'not-a-section)
                (etaf-ncm-error (list (car err) (cadr err))))
              (plist-get (etaf-ncm-state app) :section))))))
"####,
            expect![[
                r#"OK (:daily (:recent 3) (etaf-ncm-error "Unknown library: not-a-section") :recent)"#
            ]],
        ),
        // Mounting paints the library into a buffer; the player is then
        // closed, which owns and kills that buffer.
        ParityBatchCase::value(
            "mount_renders",
            r####"
(progn
  (neomacs-ncm-fresh)
  (let* ((app (neomacs-ncm-session))
         (etaf-ncm-buffer-name " *ncm-parity*")
         (etaf-ncm-status-interval 3600))
    (etaf-ncm-refresh app)
    (unwind-protect
        (let ((buffer (etaf-ncm-open app)))
          (with-current-buffer buffer
            (list (buffer-name)
                  (> (buffer-size) 0)
                  (substring-no-properties
                   (buffer-substring-no-properties (point-min) (point-max))
                   0 (min 120 (buffer-size))))))
      (etaf-ncm-close)
      (list (get-buffer etaf-ncm-buffer-name)))))
"####,
            expect![[
                r#"OK (" *ncm-parity*" t " │ \n ♫   │ 每日推荐   刷新  \n   │ │ 搜索歌曲、歌手、歌单或专辑 │   播放全部  \n 荐  │   \n 台  │  ▶  曲目 1  歌手 1   3:00 ↓    \n 心  │   2  曲目 2  歌手 2 ")"#
            ]],
        ),
        // A failing backend reply lands in the session error and message;
        // player stays usable instead of signaling.
        ParityBatchCase::value(
            "backend_failure_reported",
            r####"
(progn
  (neomacs-ncm-fresh)
  (let ((app (neomacs-ncm-session)))
    (setq neomacs-ncm-fail "后台不可用")
    (etaf-ncm-refresh app)
    (let ((state (etaf-ncm-state app)))
      (list (plist-get state :error)
            (plist-get state :message)
            (plist-get state :rows-total)
            (plist-get state :section)))))
"####,
            expect![[r#"OK ("后台不可用" "后台不可用（scripted）" 0 :daily)"#]],
        ),
    ];
    assert_oracle_batch_cases(oracle(), "etaf-ncm-behavior", "etaf-ncm", &cases);
}
