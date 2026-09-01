use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, GNTP_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const GNTP_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const GNTP_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'gntp)

(defvar gntp-test-sandbox
  (file-name-as-directory (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defun gntp-test-root (name)
  (let ((root (file-name-as-directory
               (expand-file-name name gntp-test-sandbox))))
    (when (file-exists-p root) (delete-directory root t))
    (make-directory root t)
    root))

(defun gntp-test-write (path contents)
  (make-directory (file-name-directory path) t)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert contents)
    (write-region (point-min) (point-max) path nil 'silent))
  path)

(defun gntp-test-normalize-digests (message)
  (replace-regexp-in-string
   "[[:xdigit:]]\\{32\\}" "<digest>" message t t))

(defun gntp-test-wire (message)
  (replace-regexp-in-string "\r" "<CR>" message t t))
"##;

fn gntp_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(GNTP_MELPA_PIN, "gntp.el")
        .expect("prepare pinned gntp source below ./tmp")
        .with_prelude(GNTP_TEST_PRELUDE)
        .with_timeout(GNTP_TEST_TIMEOUT)
}

fn registration_message_encodes_multiple_notification_types() -> ParityBatchCase {
    ParityBatchCase::value(
        "registration_message_encodes_multiple_notification_types",
        r##"
(let ((gntp-application-name "Neomacs Checkout Monitor")
      (gntp-application-icon "http://assets.example/app.png"))
  (gntp-test-wire
   (gntp-build-message-register
    '((checkout-failed
       :display "Checkout failed"
       :enabled t
       :icon "http://assets.example/failure.png")
      ("order-shipped"
       :display "Order shipped")
      (audit-event)))))
"##,
        expect![[
            r##"OK "GNTP/1.0 REGISTER NONE<CR>\nApplication-Name: Neomacs Checkout Monitor<CR>\nNotifications-Count: 3<CR>\nApplication-Icon: http://assets.example/app.png<CR>\n<CR>\nNotification-Name: checkout-failed<CR>\nNotification-Display-Name: Checkout failed<CR>\nNotification-Enabled: True<CR>\nNotification-Icon: http://assets.example/failure.png<CR>\n<CR>\nNotification-Name: order-shipped<CR>\nNotification-Display-Name: Order shipped<CR>\n<CR>\nNotification-Name: audit-event""##
        ]],
    )
}

fn file_icons_are_embedded_after_headers_with_matching_resource_ids() -> ParityBatchCase {
    ParityBatchCase::value(
        "file_icons_are_embedded_after_headers_with_matching_resource_ids",
        r##"
(let* ((root (gntp-test-root "gntp-file-icons"))
       (app-icon (gntp-test-write (expand-file-name "app.ico" root) "APP!"))
       (notice-icon (gntp-test-write
                     (expand-file-name "failure.ico" root) "FAILURE"))
       (gntp-application-name "Fulfilment Desk")
       (gntp-application-icon app-icon)
       (message
        (gntp-build-message-register
         `((checkout-failed
            :display "Checkout failed"
            :enabled t
            :icon ,notice-icon))))
       (app-id (md5 app-icon))
       (notice-id (md5 notice-icon)))
  (list
   :uris
   (mapcar #'gntp-test-normalize-digests
           (list (gntp-app-icon-uri)
                 (gntp-notice-icon-uri
                  `(checkout-failed :icon ,notice-icon))))
   :resource-id-contract
   (list
    (string= (gntp-app-icon-uri)
             (concat "x-growl-resource://" app-id))
    (string= (gntp-notice-icon-uri
              `(checkout-failed :icon ,notice-icon))
             (concat "x-growl-resource://" notice-id)))
   :message (gntp-test-wire (gntp-test-normalize-digests message))
   :icon-data
   (mapcar #'gntp-test-wire
           (list (gntp-test-normalize-digests (gntp-app-icon-data))
                 (gntp-test-normalize-digests
                  (gntp-notice-icon-data
                   `(checkout-failed :icon ,notice-icon)))))))
"##,
        expect![[
            r##"OK (:uris ("x-growl-resource://<digest>" "x-growl-resource://<digest>") :resource-id-contract (t t) :message "GNTP/1.0 REGISTER NONE<CR>\nApplication-Name: Fulfilment Desk<CR>\nNotifications-Count: 1<CR>\nApplication-Icon: x-growl-resource://<digest><CR>\n<CR>\nNotification-Name: checkout-failed<CR>\nNotification-Display-Name: Checkout failed<CR>\nNotification-Enabled: True<CR>\nNotification-Icon: x-growl-resource://<digest><CR>\n<CR>\nIdentifier: <digest><CR>\nLength: 4<CR>\n<CR>\nAPP!<CR>\n<CR>\nIdentifier: <digest><CR>\nLength: 7<CR>\n<CR>\nFAILURE" :icon-data ("Identifier: <digest><CR>\nLength: 4<CR>\n<CR>\nAPP!" "Identifier: <digest><CR>\nLength: 7<CR>\n<CR>\nFAILURE"))"##
        ]],
    )
}

fn notification_message_preserves_fields_and_sanitizes_embedded_crlf() -> ParityBatchCase {
    ParityBatchCase::value(
        "notification_message_preserves_fields_and_sanitizes_embedded_crlf",
        r##"
(let ((gntp-application-name "Order Console"))
  (list
   :urgent
   (gntp-test-wire
    (gntp-build-message-notify
     'checkout-failed
     "Payment rejected"
     "Card declined\r\nRetry with another method\r\nAudit queued"
     2
     "http://assets.example/card.png"))
   :defaults
   (gntp-test-wire
    (gntp-build-message-notify
     "order-ready" "Ready for pickup" "Order #417"))))
"##,
        expect![[
            r##"OK (:urgent "GNTP/1.0 NOTIFY NONE<CR>\nApplication-Name: Order Console<CR>\nNotification-Name: checkout-failed<CR>\nNotification-Title: Payment rejected<CR>\nNotification-Text: Card declined\nRetry with another method\nAudit queued<CR>\nNotification-Priority: 2<CR>\nNotification-Icon: http://assets.example/card.png<CR>\n<CR>\n" :defaults "GNTP/1.0 NOTIFY NONE<CR>\nApplication-Name: Order Console<CR>\nNotification-Name: order-ready<CR>\nNotification-Title: Ready for pickup<CR>\nNotification-Text: Order #417<CR>\nNotification-Priority: 0<CR>\nNotification-Icon: <CR>\n<CR>\n")"##
        ]],
    )
}

fn public_commands_open_the_configured_connection_and_send_complete_frames() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_commands_open_the_configured_connection_and_send_complete_frames",
        r##"
(let ((gntp-application-name "Warehouse")
      (gntp-server "growl.internal")
      (gntp-server-port 24000)
      (gntp-register-alist
       '((order-ready :display "Order ready" :enabled t)))
      network-calls
      writes)
  (cl-letf (((symbol-function 'make-network-process)
             (lambda (&rest arguments)
               (push arguments network-calls)
               'gntp-test-process))
            ((symbol-function 'process-send-string)
             (lambda (process payload)
               (push (list process (gntp-test-wire payload)) writes))))
    (gntp-register)
    (gntp-notify 'packing-delayed
                 "Packing delayed"
                 "Station 3 needs help"
                 "backup-growl.internal"
                 25000 -1))
  (list :connections (nreverse network-calls)
        :writes (nreverse writes)))
"##,
        expect![[
            r##"OK (:connections ((:name "gntp" :host "growl.internal" :server nil :service 24000 :filter gntp-filter) (:name "gntp" :host "backup-growl.internal" :server nil :service 25000 :filter gntp-filter)) :writes ((gntp-test-process "GNTP/1.0 REGISTER NONE<CR>\nApplication-Name: Warehouse<CR>\nNotifications-Count: 1<CR>\n<CR>\nNotification-Name: order-ready<CR>\nNotification-Display-Name: Order ready<CR>\nNotification-Enabled: True<CR>\n<CR>\n<CR>\n") (gntp-test-process "GNTP/1.0 NOTIFY NONE<CR>\nApplication-Name: Warehouse<CR>\nNotification-Name: packing-delayed<CR>\nNotification-Title: Packing delayed<CR>\nNotification-Text: Station 3 needs help<CR>\nNotification-Priority: -1<CR>\nNotification-Icon: <CR>\n<CR>\n<CR>\n<CR>\n<CR>\n")))"##
        ]],
    )
}

fn reply_filter_accepts_success_and_surfaces_server_errors() -> ParityBatchCase {
    ParityBatchCase::value(
        "reply_filter_accepts_success_and_surfaces_server_errors",
        r##"
(let ((success
       "GNTP/1.0 -OK NONE\r\nResponse-Action: NOTIFY\r\n\r\n")
      (failure
       "GNTP/1.0 -ERROR NONE\r\nError-Code: 401\r\nError-Description: Unknown application\r\n\r\n"))
  (list
   :success (gntp-filter 'gntp-test-process success)
   :failure
   (condition-case err
       (progn (gntp-filter 'gntp-test-process failure) :not-signaled)
     (error (list (car err) (gntp-test-wire (cadr err)))))))
"##,
        expect![[
            r##"OK (:success nil :failure (error "GNTP: Something went wrong take a look at the reply:\n GNTP/1.0 -ERROR NONE<CR>\nError-Code: 401<CR>\nError-Description: Unknown application<CR>\n<CR>\n"))"##
        ]],
    )
}

#[test]
fn gntp_package_batch() {
    let cases = vec![
        registration_message_encodes_multiple_notification_types(),
        file_icons_are_embedded_after_headers_with_matching_resource_ids(),
        notification_message_preserves_fields_and_sanitizes_embedded_crlf(),
        public_commands_open_the_configured_connection_and_send_complete_frames(),
        reply_filter_accepts_success_and_surfaces_server_errors(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed gntp parity test");
    assert_oracle_batch_cases(gntp_oracle(), test_name, "gntp_parity", &cases);
}
