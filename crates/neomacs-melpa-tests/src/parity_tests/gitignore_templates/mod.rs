use std::time::Duration;

use crate::{CachedMelpaOracle, GITIGNORE_TEMPLATES_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const GITIGNORE_TEMPLATES_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const GITIGNORE_TEMPLATES_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)

;; Recorded 2026-08-10 from the official GitHub REST API.  The decoded
;; response bodies are byte-for-byte fixtures, not a rolling network oracle.
;; List: 2080 bytes, sha256 8654753a0e86c1e193c7b62debbde5979ad3ebcdc1a738265f6663cb2ea08747.
;; Rust: 742 bytes, sha256 041f805e49d991e9d3f742a84ee23fe6adc39749afbbb0c6e2e7bd8d699fd5ee.
(defconst gitignore-test-github-list-body
  (decode-coding-string
   (base64-decode-string
    "WwogICJBTCIsCiAgIkFjdGlvbnNjcmlwdCIsCiAgIkFkYSIsCiAgIkFkdmVudHVyZUdhbWVTdHVkaW8iLAogICJBZ2RhIiwKICAiQW5kcm9pZCIsCiAgIkFuZ3VsYXIiLAogICJBcHBFbmdpbmUiLAogICJBcHBjZWxlcmF0b3JUaXRhbml1bSIsCiAgIkFyY2hMaW51eFBhY2thZ2VzIiwKICAiQXV0b3Rvb2xzIiwKICAiQmFsbGVyaW5hIiwKICAiQyIsCiAgIkMrKyIsCiAgIkNGV2hlZWxzIiwKICAiQ01ha2UiLAogICJDVURBIiwKICAiQ2FrZVBIUCIsCiAgIkNoZWZDb29rYm9vayIsCiAgIkNsb2p1cmUiLAogICJDb2RlSWduaXRlciIsCiAgIkNvbW1vbkxpc3AiLAogICJDb21wb3NlciIsCiAgIkNvbmNyZXRlNSIsCiAgIkNvcSIsCiAgIkNyYWZ0Q01TIiwKICAiRCIsCiAgIkRNIiwKICAiRGFydCIsCiAgIkRlbHBoaSIsCiAgIkRlbm8iLAogICJEb3RuZXQiLAogICJEcnVwYWwiLAogICJFUGlTZXJ2ZXIiLAogICJFYWdsZSIsCiAgIkVsaXNwIiwKICAiRWxpeGlyIiwKICAiRWxtIiwKICAiRXJsYW5nIiwKICAiRXhwcmVzc2lvbkVuZ2luZSIsCiAgIkV4dEpzIiwKICAiRmFuY3kiLAogICJGaW5hbGUiLAogICJGaXJlYmFzZSIsCiAgIkZsYXhFbmdpbmUiLAogICJGbHV0dGVyIiwKICAiRm9yY2VEb3RDb20iLAogICJGb3J0cmFuIiwKICAiRnVlbFBIUCIsCiAgIkdXVCIsCiAgIkdjb3YiLAogICJHaXRCb29rIiwKICAiR2l0SHViUGFnZXMiLAogICJHbGVhbSIsCiAgIkdvIiwKICAiR29kb3QiLAogICJHcmFkbGUiLAogICJHcmFpbHMiLAogICJISVAiLAogICJIYXNrZWxsIiwKICAiSGF4ZSIsCiAgIklBUiIsCiAgIklHT1JQcm8iLAogICJJZHJpcyIsCiAgIkpCb3NzIiwKICAiSkVOS0lOU19IT01FIiwKICAiSmF2YSIsCiAgIkpla3lsbCIsCiAgIkpvb21sYSIsCiAgIkp1bGlhIiwKICAiS2F0YWxvbiIsCiAgIktpQ2FkIiwKICAiS29oYW5hIiwKICAiS290bGluIiwKICAiTGFiVklFVyIsCiAgIkxhbmdDaGFpbiIsCiAgIkxhcmF2ZWwiLAogICJMZWFuIiwKICAiTGVpbmluZ2VuIiwKICAiTGVtb25TdGFuZCIsCiAgIkxpbHlwb25kIiwKICAiTGl0aGl1bSIsCiAgIkx1YSIsCiAgIkx1YXUiLAogICJNYWdlbnRvIiwKICAiTWF2ZW4iLAogICJNZXJjdXJ5IiwKICAiTWV0YVByb2dyYW1taW5nU3lzdGVtIiwKICAiTW9kZWxTaW0iLAogICJNb2RlbGljYSIsCiAgIk5hbm9jIiwKICAiTmVzdGpzIiwKICAiTmV4dGpzIiwKICAiTmltIiwKICAiTml4IiwKICAiTm9kZSIsCiAgIk9DYW1sIiwKICAiT2JqZWN0aXZlLUMiLAogICJPcGEiLAogICJPcGVuQ2FydCIsCiAgIk9yYWNsZUZvcm1zIiwKICAiUGFja2VyIiwKICAiUGVybCIsCiAgIlBoYWxjb24iLAogICJQbGF5RnJhbWV3b3JrIiwKICAiUGxvbmUiLAogICJQcmVzdGFzaG9wIiwKICAiUHJvY2Vzc2luZyIsCiAgIlB1cmVTY3JpcHQiLAogICJQeXRob24iLAogICJRb294ZG9vIiwKICAiUXQiLAogICJSIiwKICAiUk9TIiwKICAiUmFja2V0IiwKICAiUmFpbHMiLAogICJSYWt1IiwKICAiUmVTY3JpcHQiLAogICJSaG9kZXNSaG9tb2JpbGUiLAogICJSdWJ5IiwKICAiUnVzdCIsCiAgIlNDb25zIiwKICAiU1NEVC1zcWxwcm9qIiwKICAiU2FsZXNmb3JjZSIsCiAgIlNhc3MiLAogICJTY2FsYSIsCiAgIlNjaGVtZSIsCiAgIlNjcml2ZW5lciIsCiAgIlNkY2MiLAogICJTZWFtR2VuIiwKICAiU2tldGNoVXAiLAogICJTbWFsbHRhbGsiLAogICJTb2xpZFdvcmtzIiwKICAiU29saWRpdHktUmVtaXgiLAogICJTdGVsbGEiLAogICJTdWdhckNSTSIsCiAgIlN3aWZ0IiwKICAiU3ltZm9ueSIsCiAgIlN5bXBob255Q01TIiwKICAiVGVYIiwKICAiVGVycmFmb3JtIiwKICAiVGVzdENvbXBsZXRlIiwKICAiVGV4dHBhdHRlcm4iLAogICJUdXJib0dlYXJzMiIsCiAgIlR3aW5DQVQzIiwKICAiVHlwbzMiLAogICJVbml0eSIsCiAgIlVucmVhbEVuZ2luZSIsCiAgIlZCQSIsCiAgIlZWVlYiLAogICJWaXN1YWxTdHVkaW8iLAogICJXYWYiLAogICJXb3JkUHJlc3MiLAogICJYb2pvIiwKICAiWWVvbWFuIiwKICAiWWlpIiwKICAiWmVuZEZyYW1ld29yayIsCiAgIlplcGhpciIsCiAgIlppZyIsCiAgImJ1biIsCiAgImVjdS50ZXN0IgpdCg==")
   'utf-8))

(defconst gitignore-test-github-rust-body
  (decode-coding-string
   (base64-decode-string
    "ewogICJuYW1lIjogIlJ1c3QiLAogICJzb3VyY2UiOiAiIyBHZW5lcmF0ZWQgYnkgQ2FyZ29cbiMgd2lsbCBoYXZlIGNvbXBpbGVkIGZpbGVzIGFuZCBleGVjdXRhYmxlc1xuZGVidWdcbnRhcmdldFxuXG4jIFRoZXNlIGFyZSBiYWNrdXAgZmlsZXMgZ2VuZXJhdGVkIGJ5IHJ1c3RmbXRcbioqLyoucnMuYmtcblxuIyBNU1ZDIFdpbmRvd3MgYnVpbGRzIG9mIHJ1c3RjIGdlbmVyYXRlIHRoZXNlLCB3aGljaCBzdG9yZSBkZWJ1Z2dpbmcgaW5mb3JtYXRpb25cbioucGRiXG5cbiMgR2VuZXJhdGVkIGJ5IGNhcmdvIG11dGFudHNcbiMgQ29udGFpbnMgbXV0YXRpb24gdGVzdGluZyBkYXRhXG4qKi9tdXRhbnRzLm91dCovXG5cbiMgUnVzdFJvdmVyXG4jICBKZXRCcmFpbnMgc3BlY2lmaWMgdGVtcGxhdGUgaXMgbWFpbnRhaW5lZCBpbiBhIHNlcGFyYXRlIEpldEJyYWlucy5naXRpZ25vcmUgdGhhdCBjYW5cbiMgIGJlIGZvdW5kIGF0IGh0dHBzOi8vZ2l0aHViLmNvbS9naXRodWIvZ2l0aWdub3JlL2Jsb2IvbWFpbi9HbG9iYWwvSmV0QnJhaW5zLmdpdGlnbm9yZVxuIyAgYW5kIGNhbiBiZSBhZGRlZCB0byB0aGUgZ2xvYmFsIGdpdGlnbm9yZSBvciBtZXJnZWQgaW50byB0aGlzIGZpbGUuICBGb3IgYSBtb3JlIG51Y2xlYXJcbiMgIG9wdGlvbiAobm90IHJlY29tbWVuZGVkKSB5b3UgY2FuIHVuY29tbWVudCB0aGUgZm9sbG93aW5nIHRvIGlnbm9yZSB0aGUgZW50aXJlIGlkZWEgZm9sZGVyLlxuIy5pZGVhL1xuIgp9Cg==")
   'utf-8))

;; The legacy gitignore.io URLs hard-coded by the pinned package redirect to
;; the official Toptal successor.  These are the complete official list and
;; Rust bodies recorded from that successor on 2026-08-10.
;; List: 4941 bytes, sha256 c45471cc979c647279149bfa3e5544d8cdaa2c5d1bca94737ea1ad380200b885.
;; Rust: 626 bytes, sha256 d192d67fece05ceafd683a251c3c82f4c246482158a39ac1abd5a714d23c6212.
(defconst gitignore-test-gitignore-io-list-body
  (decode-coding-string
   (base64-decode-string
    "MWMsMWMtYml0cml4LGEtZnJhbWUsYWN0aW9uc2NyaXB0LGFkYQphZG9iZSxhZHZhbmNlZGluc3RhbGxlcixhZHZlbnR1cmVnYW1lc3R1ZGlvLGFnZGEsYWwKYWx0ZXJhcXVhcnR1c2lpLGFsdGl1bSxhbXBsaWZ5LGFuZHJvaWQsYW5kcm9pZHN0dWRpbwphbmd1bGFyLGFuanV0YSxhbnNpYmxlLGFuc2libGV0b3dlcixhcGFjaGVjb3Jkb3ZhCmFwYWNoZWhhZG9vcCxhcHBidWlsZGVyLGFwcGNlbGVyYXRvcnRpdGFuaXVtLGFwcGNvZGUsYXBwY29kZSthbGwKYXBwY29kZStpbWwsYXBwZW5naW5lLGFwdGFuYXN0dWRpbyxhcmNhbmlzdCxhcmNoaXZlCmFyY2hpdmVzLGFyY2hsaW51eHBhY2thZ2VzLGFzZGYsYXNwbmV0Y29yZSxhc3NlbWJsZXIKYXN0cm8sYXRlLGF0bWVsc3R1ZGlvLGF0cyxhdWRpbwphdXRvaG90a2V5LGF1dG9tYXRpb25zdHVkaW8sYXV0b3Rvb2xzLGF1dG90b29scytzdHJpY3QsYXdyCmF6dXJlZnVuY3Rpb25zLGF6dXJpdGUsYmFja3VwLGJhbGxlcmluYSxiYXNlcmNtcwpiYXNpYyxiYXRjaCxiYXphYXIsYmF6ZWwsYml0cmlzZQpiaXRyaXgsYml0dG9ycmVudCxibGFja2JveCxibGVuZGVyLGJsb29wCmJsdWVqLGJvb2tkb3duLGJvd2VyLGJyaWN4Y2MsYnVjawpjLGMrKyxjYWtlLGNha2VwaHAsY2FrZXBocDIKY2FrZXBocDMsY2FsYWJhc2gsY2FydGhhZ2UsY2VydGlmaWNhdGVzLGNleWxvbgpjZndoZWVscyxjaGVmY29va2Jvb2ssY2hvY29sYXRleSxjaXJjdWl0cHl0aG9uLGNsZWFuCmNsaW9uLGNsaW9uK2FsbCxjbGlvbitpbWwsY2xvanVyZSxjbG91ZDkKY21ha2UsY29jb2Fwb2RzLGNvY29zMmR4LGNvY29zY3JlYXRvcixjb2RlYmxvY2tzCmNvZGVjb21wb3NlcnN0dWRpbyxjb2RlaWduaXRlcixjb2RlaW8sY29kZWtpdCxjb2Rlc25pZmZlcgpjb2ZmZWVzY3JpcHQsY29tbW9ubGlzcCxjb21wb2RvYyxjb21wb3Nlcixjb21wcmVzc2VkCmNvbXByZXNzZWRhcmNoaXZlLGNvbXByZXNzaW9uLGNvbmFuLGNvbmNyZXRlNSxjb3EKY29yZG92YSxjcmFmdGNtcyxjcmFzaGx5dGljcyxjcmJhc2ljLGNyb3NzYmFyCmNyeXN0YWwsY3MtY2FydCxjc2hhcnAsY3VkYSxjdnMKY3lwcmVzc2lvLGQsZGFydCxkYXJ0ZWRpdG9yLGRhdGEKZGF0YWJhc2UsZGF0YXJlY292ZXJ5LGRiZWF2ZXIsZGJ0LGRlZm9sZApkZWxwaGksZGVubyxkZnJhbWUsZGlmZixkaXJlbnYKZGlza2ltYWdlLGRqYW5nbyxkbSxkb2NmeCxkb2NwcmVzcwpkb2N1c2F1cnVzLGRvY3osZG90ZW52LGRvdGZpbGVzc2gsZG90bmV0Y29yZQpkb3RzZXR0aW5ncyxkb3h5Z2VuLGRyZWFtd2VhdmVyLGRyb3Bib3gsZHJ1cGFsCmRydXBhbDcsZHJ1cGFsOCxlMnN0dWRpbyxlYWdsZSxlYXN5Ym9vawplY2xpcHNlLGVpZmZlbHN0dWRpbyxlbGFzdGljYmVhbnN0YWxrLGVsaXNwLGVsaXhpcgplbG0sZW1hY3MsZW1iZXIsZW5zaW1lLGVwaXNlcnZlcgplcmxhbmcsZXNwcmVzc28sZXhlY3V0YWJsZSxleGVyY2lzbSxleHByZXNzaW9uZW5naW5lCmV4dGpzLGZhbmN5LGZhc3RsYW5lLGZpbmFsZSxmaXJlYmFzZQpmaXNoLGZsYXNoYnVpbGRlcixmbGFzayxmbGF0cGFrLGZsZXgKZmxleGJ1aWxkZXIsZmxvb2JpdHMsZmx1dHRlcixmb250LGZvbnRmb3JnZQpmb3JjZWRvdGNvbSxmb3JnZWdyYWRsZSxmb3J0cmFuLGZyZWVjYWQsZnJlZXBhc2NhbApmc2hhcnAsZnVlbHBocCxmdXNldG9vbHMsZ2FtZXMsZ2F0c2J5Cmdjb3YsZ2VuZXJvNGdsLGdldGgsZ2d0cyxnaXMKZ2l0LGdpdGJvb2ssZ28sZ29kb3QsZ29sYW5kCmdvbGFuZCthbGwsZ29sYW5kK2ltbCxnb29kc3luYyxncGcsZ3JhZGxlCmdyYWlscyxncmVlbmZvb3QsZ3Jvb3Z5LGdydW50LGd3dApoYXNrZWxsLGhlbG0saGV4byxob2wsaG9tZWFzc2lzdGFudApob21lYnJldyxoc3AsaHVnbyxoeXBlcmxlZGdlcmNvbXBvc2VyLGlhcgppYXJfZXdhcm0saWFyZW1iZWRkZWR3b3JrYmVuY2gsaWRhcHJvLGlkcmlzLGlnb3Jwcm8KaW1hZ2VzLGluZmVyLGluZm9yY21zLGluZm9yY3JtLGludGVsbGlqCmludGVsbGlqK2FsbCxpbnRlbGxpaitpbWwsaW9uaWMzLGphYnJlZixqYW5ldApqYXZhLGpib3NzLGpib3NzLTQtMi0zLWdhLGpib3NzLTYteCxqYm9zczQKamJvc3M2LGpkZXZlbG9wZXIsamVreWxsLGplbnYsamV0YnJhaW5zCmpldGJyYWlucythbGwsamV0YnJhaW5zK2ltbCxqZ2l2ZW4samlnc2F3LGptZXRlcgpqb2Usam9vbWxhLGpzb25uZXQsanNwbSxqdWxpYQpqdXB5dGVybm90ZWJvb2tzLGp1c3Rjb2RlLGthbGRpLGthdGUsa2RldmVsb3A0CmtkaWZmMyxrZWlsLGtlbnRpY28sa2V5c3RvbmVqcyxraWNhZApraXJieTIsa2lyYnkzLGtvYmFsdCxrb2hhbmEsa29tb2RvZWRpdAprb255dmlzdWFsaXplcixrb3RsaW4sbGFidmlldyxsYWJ2aWV3bnhnLGxhbXAKbGFyYXZlbCxsYXRleCxsYXphcnVzLGxlaW5pbmdlbixsZW1vbnN0YW5kCmxlc3MsbGliZXJvc29jLGxpYnJhcmlhbi1jaGVmLGxpYnJlb2ZmaWNlLGxpZ2h0aG91c2VjaQpsaWx5cG9uZCxsaW51eCxsaXRoaXVtLGxvY2Fsc3RhY2ssbG9ndGFsawpsc3NwaWNlLGx0c3BpY2UsbHVhLGx5eCxtYWNvcwptYWdlbnRvLG1hZ2VudG8xLG1hZ2VudG8yLG1hZ2ljLXhwYSxtYXRsYWIKbWF2ZW4sbWF2ZW5zbWF0ZSxtZGJvb2ssbWVhbixtZXJjdXJpYWwKbWVyY3VyeSxtZXNvbixtZXRhbHMsbWV0YWxzbWl0aCxtZXRhcHJvZ3JhbW1pbmdzeXN0ZW0KbWV0ZW9yLG1ldGVvcmpzLG1pY3Jvc29mdG9mZmljZSxtaWtyb2MsbWlsbAptb2Jhbixtb2RlbHNpbSxtb2R4LG1vbWVudGljcyxtb25vZGV2ZWxvcAptcGxhYngsbXVsZSxuYW5vYyxuYXRpdmVzY3JpcHQsbmNydW5jaApuZXNjLG5ldGJlYW5zLG5ldHRlLG5leHRqcyxuaWtvbGEKbmltLG5pbmphLG5vZGUsbm9kZWNoYWtyYXRpbWV0cmF2ZWxkZWJ1Zyxub2h1cApub3RlcGFkcHAsbm92YSxub3csbnV4dGpzLG53anMKb2JqZWN0aXZlLWMsb2JzaWRpYW4sb2NhbWwsb2N0YXZlLG9jdG9iZXJjbXMKb3BhLG9wZW5jYXJ0LG9wZW5jdixvcGVuZm9hbSxvcGVuZnJhbWV3b3JrcwpvcGVuZnJhbWV3b3Jrcyt2aXN1YWxzdHVkaW8sb3JhY2xlZm9ybXMsb3JjYWQsb3N4LG90dG8Kb3hpZGVzaG9wLG94eWdlbnhtbGVkaXRvcixwYWNrZXIscGFudHMscGFydGljbGUKcGF0Y2gscGF3bixwZXJsLHBlcmw2LHBoN2NtcwpwaGFsY29uLHBob2VuaXgscGhwLWNzLWZpeGVyLHBocGNvZGVzbmlmZmVyLHBocHN0b3JtCnBocHN0b3JtK2FsbCxwaHBzdG9ybStpbWwscGhwdW5pdCxwaWNvOCxwaW1jb3JlCnBpbWNvcmU0LHBpbWNvcmU1LHBpbmVncm93LHBsYXRmb3JtaW8scGxheWZyYW1ld29yawpwbG9uZSxwb2x5bWVyLHBvd2Vyc2hlbGwscHJlbWFrZS1nbWFrZSxwcmVwcm9zCnByZXN0YXNob3AscHJvY2Vzc2luZyxwcm9ncmVzc2FibCxwc29jY3JlYXRvcixwdWx1bWkKcHVsdW1pK3N0YWNrcyxwdXBwZXQscHVwcGV0LWxpYnJhcmlhbixwdXJlYmFzaWMscHVyZXNjcmlwdApwdXR0eSxwdnMscHljaGFybSxweWNoYXJtK2FsbCxweWNoYXJtK2ltbApweWRldixweXRob24scHl0aG9udmFuaWxsYSxxbWwscW9veGRvbwpxdCxxdGNyZWF0b3IscixyYWNrZXQscmFpbHMKcmVhY3QscmVhY3RuYXRpdmUscmVhc29ubWwscmVkLHJlZGNhcgpyZWRpcyxyZW1peCxyZW1peCthcmMscmVtaXgrY2xvdWRmbGFyZXBhZ2VzLHJlbWl4K2Nsb3VkZmxhcmV3b3JrZXJzCnJlbWl4K25ldGxpZnkscmVtaXgrdmVyY2VsLHJlbnB5LHJlcGxpdCxyZXRvb2wKcmhvZGVzcmhvbW9iaWxlLHJpZGVyLHJvYm90ZnJhbWV3b3JrLHJvb3Qscm9zCnJvczIscnVieSxydWJ5bWluZSxydWJ5bWluZSthbGwscnVieW1pbmUraW1sCnJ1c3QscnVzdC1hbmFseXplcixzYWxlc2ZvcmNlLHNhbGVzZm9yY2VkeCxzYW0Kc2FtK2NvbmZpZyxzYXMsc2FzcyxzYnQsc2NhbGEKc2NoZW1lLHNjb25zLHNjcml2ZW5lcixzZGNjLHNlYW1nZW4Kc2VuY2hhdG91Y2gsc2VydmVybGVzcyxzaG9wd2FyZSxzaWx2ZXJzdHJpcGUsc2tldGNodXAKc2xpY2tlZGl0LHNtYWxsdGFsayxzbmFwLHNuYXBjcmFmdCxzbnlrCnNvbGlkaXR5LHNvbGlkaXR5dHJ1ZmZsZSxzb25hcixzb25hcnF1YmUsc291cmNlcGF3bgpzcGFyayxzcGVjZmxvdyxzcGx1bmssc3ByZWFkc2hlZXQsc3NoCnN0YW5kYXJkbWwsc3RhdGEsc3RkbGliLHN0ZWxsYSxzdGVsbGFyCnN0b3J5Ym9va2pzLHN0cmFwaSxzdHlsdXMsc3VibGltZXRleHQsc3VnYXJjcm0Kc3ZlbHRlLHN2bixzd2lmdCxzd2lmdHBhY2thZ2VtYW5hZ2VyLHN3aWZ0cG0Kc3ltZm9ueSxzeW1waG9ueWNtcyxzeW5vbG9neSxzeW5vcHN5c3Zjcyx0YWdzCnRhcm1haW5zdGFsbG1hdGUsdGVycmFmb3JtLHRlcnJhZ3J1bnQsdGVzdCx0ZXN0Y29tcGxldGUKdGVzdGluZnJhLHRleCx0ZXh0LHRleHRtYXRlLHRleHRwYXR0ZXJuCnRoZW9zLXR3ZWFrLHRoaW5rcGhwLHRsYSssdG9pdCx0b3J0b2lzZWdpdAp0b3dlcix0dXJibyx0dXJib2dlYXJzMix0d2luY2F0Myx0eWUKdHlwaW5ncyx0eXBvMyx0eXBvMy1jb21wb3Nlcix1bWJyYWNvLHVuaXR5CnVucmVhbGVuZ2luZSx2YWFkaW4sdmFncmFudCx2YWxncmluZCx2YXBvcgp2Y3BrZyx2ZW52LHZlcmNlbCx2ZXJ0eCx2aWRlbwp2aW0sdmlydHVhbGVudix2aXJ0dW9zbyx2aXN1YWxiYXNpYyx2aXN1YWxzdHVkaW8KdmlzdWFsc3R1ZGlvY29kZSx2aXZhZG8sdmxhYix2cmVhbGl6ZW9yY2hlc3RyYXRvcix2cwp2dWUsdnVlanMsdnZ2dix3YWYsd2FrYW5kYQp3ZWIsd2VibWV0aG9kcyx3ZWJzdG9ybSx3ZWJzdG9ybSthbGwsd2Vic3Rvcm0raW1sCndlcmNrZXJjbGksd2luZG93cyx3aW50ZXJzbWl0aCx3b3JkcHJlc3Msd3lhbQp4YW1hcmluc3R1ZGlvLHhjb2RlLHhjb2RlaW5qZWN0aW9uLHhpbGlueCx4aWxpbnhpc2UKeGlsaW54dml2YWRvLHhpbGwseG1ha2UseG9qbyx4dGV4dAp5ODYseWFsYyx5YXJuLHllb21hbix5aWkKeWlpMix6ZW5kZnJhbWV3b3JrLHplcGhpcix6aWcsenNoCnp1a2VuY3I4MDAw")
   'utf-8))

(defconst gitignore-test-gitignore-io-rust-body
  (decode-coding-string
   (base64-decode-string
    "IyBDcmVhdGVkIGJ5IGh0dHBzOi8vd3d3LnRvcHRhbC5jb20vZGV2ZWxvcGVycy9naXRpZ25vcmUvYXBpL3J1c3QKIyBFZGl0IGF0IGh0dHBzOi8vd3d3LnRvcHRhbC5jb20vZGV2ZWxvcGVycy9naXRpZ25vcmU/dGVtcGxhdGVzPXJ1c3QKCiMjIyBSdXN0ICMjIwojIEdlbmVyYXRlZCBieSBDYXJnbwojIHdpbGwgaGF2ZSBjb21waWxlZCBmaWxlcyBhbmQgZXhlY3V0YWJsZXMKZGVidWcvCnRhcmdldC8KCiMgUmVtb3ZlIENhcmdvLmxvY2sgZnJvbSBnaXRpZ25vcmUgaWYgY3JlYXRpbmcgYW4gZXhlY3V0YWJsZSwgbGVhdmUgaXQgZm9yIGxpYnJhcmllcwojIE1vcmUgaW5mb3JtYXRpb24gaGVyZSBodHRwczovL2RvYy5ydXN0LWxhbmcub3JnL2NhcmdvL2d1aWRlL2NhcmdvLXRvbWwtdnMtY2FyZ28tbG9jay5odG1sCkNhcmdvLmxvY2sKCiMgVGhlc2UgYXJlIGJhY2t1cCBmaWxlcyBnZW5lcmF0ZWQgYnkgcnVzdGZtdAoqKi8qLnJzLmJrCgojIE1TVkMgV2luZG93cyBidWlsZHMgb2YgcnVzdGMgZ2VuZXJhdGUgdGhlc2UsIHdoaWNoIHN0b3JlIGRlYnVnZ2luZyBpbmZvcm1hdGlvbgoqLnBkYgoKIyBFbmQgb2YgaHR0cHM6Ly93d3cudG9wdGFsLmNvbS9kZXZlbG9wZXJzL2dpdGlnbm9yZS9hcGkvcnVzdAo=")
   'utf-8))

(defconst gitignore-test-github-missing-body
  (decode-coding-string
   (base64-decode-string
    "ewogICJtZXNzYWdlIjogIk5vdCBGb3VuZCIsCiAgImRvY3VtZW50YXRpb25fdXJsIjogImh0dHBzOi8vZG9jcy5naXRodWIuY29tL3YzL2dpdGlnbm9yZSIsCiAgInN0YXR1cyI6ICI0MDQiCn0K")
   'utf-8))

(defvar gitignore-test-fixtures nil)
(defvar gitignore-test-requests nil)
(defvar gitignore-test-response-buffers nil)
(defvar gitignore-test-owned-buffers nil)
(defvar gitignore-test-completion-choice nil)
(defvar gitignore-test-completions nil)

(defun gitignore-test-fixture (id url headers status reason content-type body)
  (list :id id :url url :headers headers :status status
        :reason reason :content-type content-type :body body))

(defun gitignore-test-outcome (id url headers kind value)
  (list :id id :url url :headers headers :kind kind :value value))

(defun gitignore-test-retrieve (url &rest arguments)
  "Serve one strict recorded URL response at the real url.el buffer seam."
  (let ((fixture (pop gitignore-test-fixtures)))
    (unless fixture
      (error "Unexpected external URL request: %S" url))
    (unless (equal arguments nil)
      (error "Unexpected URL transport arguments: %S" arguments))
    (unless (equal url (plist-get fixture :url))
      (error "URL mismatch: expected %S, got %S"
             (plist-get fixture :url) url))
    (unless (equal url-request-method "GET")
      (error "Method mismatch: %S" url-request-method))
    (unless (equal url-request-extra-headers (plist-get fixture :headers))
      (error "Header mismatch for %S: expected %S, got %S"
             url (plist-get fixture :headers) url-request-extra-headers))
    (unless (null url-request-data)
      (error "Unexpected request data for %S: %S" url url-request-data))
    (push (list :id (plist-get fixture :id)
                :url url :method url-request-method
                :headers (copy-tree url-request-extra-headers)
                :data url-request-data :arguments arguments)
          gitignore-test-requests)
    (pcase (plist-get fixture :kind)
      ('return-nil nil)
      ('signal
       (let ((condition (plist-get fixture :value)))
         (signal (car condition) (cdr condition))))
      (_
       (let* ((body-bytes
               (encode-coding-string (plist-get fixture :body) 'utf-8))
              (buffer
               (generate-new-buffer
                (format " *gitignore-response-%s*"
                        (plist-get fixture :id))))
              marker)
         (push buffer gitignore-test-response-buffers)
         (with-current-buffer buffer
           (set-buffer-multibyte nil)
           (insert
            (format "HTTP/1.1 %d %s\r\nContent-Type: %s\r\nContent-Length: %d\r\nX-Neomacs-Recorded-Fixture: %s\r\n\r\n"
                    (plist-get fixture :status)
                    (plist-get fixture :reason)
                    (plist-get fixture :content-type)
                    (string-bytes body-bytes)
                    (plist-get fixture :id)))
           (setq marker (copy-marker (1- (point))))
           (set (make-local-variable 'url-http-end-of-headers) marker)
           (set (make-local-variable 'url-http-response-status)
                (plist-get fixture :status))
           (insert body-bytes)
           (goto-char (point-min)))
         buffer)))))

(defun gitignore-test-completing-read
    (prompt collection predicate require-match
            &optional initial-input history default inherit-input-method)
  "Observe the real public command's completion contract."
  (unless (equal prompt ".gitignore template: ")
    (error "Unexpected completion prompt: %S" prompt))
  (unless (null predicate)
    (error "Unexpected completion predicate: %S" predicate))
  (unless (eq require-match t)
    (error "Completion is not require-match: %S" require-match))
  (unless (member gitignore-test-completion-choice collection)
    (error "Scripted completion is absent: %S" gitignore-test-completion-choice))
  (push
   (list :prompt prompt :count (length collection)
         :first (seq-take collection 4)
         :last (copy-sequence (last collection 4))
         :collection-sha256 (secure-hash 'sha256 (prin1-to-string collection))
         :predicate predicate :require-match require-match
         :initial initial-input :history history :default default
         :inherit inherit-input-method :choice gitignore-test-completion-choice)
   gitignore-test-completions)
  gitignore-test-completion-choice)

(defun gitignore-test-buffer (name)
  (when (get-buffer name)
    (error "Owned buffer already exists: %s" name))
  (let ((buffer (generate-new-buffer name)))
    (push buffer gitignore-test-owned-buffers)
    buffer))

(defun gitignore-test-capture (function)
  "Return FUNCTION's value or its exact nonlocal condition."
  (condition-case condition
      (list :value (funcall function))
    (t (list :signal (car condition) :data (cdr condition)))))

(defun gitignore-test-file-string (file)
  "Read FILE as ordinary decoded user-visible text without visiting it."
  (with-temp-buffer
    (insert-file-contents file)
    (buffer-string)))

(defun gitignore-test-recording (body)
  "Return immutable BODY recording identity."
  (list :characters (length body) :bytes (string-bytes body)
        :sha256 (secure-hash 'sha256 body)))

(defun gitignore-test-run (name api fixtures function)
  "Run FUNCTION with strict transport, filesystem, and buffer ownership."
  (let ((sandbox-root (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
    (unless (and (stringp sandbox-root)
                 (not (string-empty-p sandbox-root))
                 (file-name-absolute-p sandbox-root))
      (error "NEOMACS_TEST_SANDBOX_ROOT must be a nonempty absolute path"))
    (let* ((root (file-name-as-directory (expand-file-name name sandbox-root)))
           (gitignore-templates-api api)
           (gitignore-templates-names nil)
           (gitignore-templates-alist nil)
           (gitignore-test-fixtures (copy-tree fixtures))
           (gitignore-test-requests nil)
           (gitignore-test-response-buffers nil)
           (gitignore-test-owned-buffers nil)
           (gitignore-test-completion-choice nil)
           (gitignore-test-completions nil)
           result transport cleanup first-error)
      (when (file-exists-p root)
        (delete-directory root t))
      (make-directory root t)
      (condition-case error-data
          (save-window-excursion
            (save-current-buffer
              (let ((default-directory root))
                (cl-letf (((symbol-function 'url-retrieve-synchronously)
                           #'gitignore-test-retrieve))
                  (setq result (funcall function root))))))
        (error (setq first-error error-data)))
      (setq transport
            (list :requests (nreverse gitignore-test-requests)
                  :unused (length gitignore-test-fixtures)
                  :response-live
                  (mapcar #'buffer-live-p
                          (reverse gitignore-test-response-buffers))
                  :completions (nreverse gitignore-test-completions)))
      (when (and gitignore-test-fixtures (not first-error))
        (setq first-error
              (list 'error
                    (format "Unused URL fixtures: %S"
                            (mapcar (lambda (fixture) (plist-get fixture :id))
                                    gitignore-test-fixtures)))))
      (dolist (buffer (append gitignore-test-owned-buffers
                              gitignore-test-response-buffers))
        (condition-case error-data
            (when (buffer-live-p buffer) (kill-buffer buffer))
          (error (unless first-error (setq first-error error-data)))))
      (condition-case error-data
          (when (file-exists-p root) (delete-directory root t))
        (error (unless first-error (setq first-error error-data))))
      (setq cleanup
            (list :owned-live
                  (and (seq-some #'buffer-live-p gitignore-test-owned-buffers) t)
                  :response-live
                  (and (seq-some #'buffer-live-p
                                 gitignore-test-response-buffers) t)
                  :root-exists (file-exists-p root)))
      (setq gitignore-test-fixtures nil
            gitignore-test-requests nil
            gitignore-test-response-buffers nil
            gitignore-test-owned-buffers nil
            gitignore-test-completions nil)
      (when first-error
        (signal (car first-error) (cdr first-error)))
      (list :result result :transport transport :cleanup cleanup))))
"####;

fn gitignore_templates_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(GITIGNORE_TEMPLATES_MELPA_PIN, "gitignore-templates.el")
        .expect("prepare exact gitignore-templates source below ./tmp")
        .with_prelude(GITIGNORE_TEMPLATES_TEST_PRELUDE)
        .with_timeout(GITIGNORE_TEMPLATES_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed gitignore-templates parity test")
        .into()
}

fn assert_gitignore_templates_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        gitignore_templates_oracle(),
        &current_test_name(),
        "gitignore_templates_parity",
        cases,
    );
}

#[test]
fn gitignore_templates_package_batch() {
    assert_gitignore_templates_batch(&workflows::public_workflow_cases());
}
