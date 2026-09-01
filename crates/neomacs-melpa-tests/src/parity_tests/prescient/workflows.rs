use expect_test::expect;

use super::ParityBatchCase;

/// The configuration surface and payload: the documented defcustoms with
/// defaults and types, including the filter-method list and the
/// filter/char-folding policies.
fn the_configuration_surface_and_payload() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_configuration_surface_and_payload",
        r####"(unwind-protect
    (progn
      (pr564-test-reset)
      (list
       :source (pr564-test-source-state)
       :options
       (mapcar
        (lambda (option)
          (list :option option
                :custom-variable-p (and (custom-variable-p option) t)
                :standard (eval (car (get option 'standard-value)))
                :type (get option 'custom-type)))
        '(prescient-history-length
          prescient-frequency-decay
          prescient-frequency-threshold
          prescient-filter-method
          prescient-sort-length-enable
          prescient-sort-full-matches-first
          prescient-use-char-folding
          prescient-use-case-folding
          prescient-aggressive-file-save))))
  (pr564-test-reset))"####,
        expect![[
            r#"OK (:source (:upstream-tree "ba7d18e7cbfc4e6483ce786b6e1698d065ed9499" :feature t :version "20260628.2243") :options ((:option prescient-history-length :custom-variable-p t :standard 100 :type number) (:option prescient-frequency-decay :custom-variable-p t :standard 0.997 :type number) (:option prescient-frequency-threshold :custom-variable-p t :standard 0.05 :type number) (:option prescient-filter-method :custom-variable-p t :standard (literal regexp initialism) :type (set (const :tag "Literal" literal) (const :tag "Literal Prefix" literal-prefix) (const :tag "Regexp" regexp) (const :tag "Initialism" initialism) (const :tag "Fuzzy" fuzzy) (const :tag "Prefix" prefix) (const :tag "Anchored" anchored))) (:option prescient-sort-length-enable :custom-variable-p t :standard t :type boolean) (:option prescient-sort-full-matches-first :custom-variable-p t :standard nil :type boolean) (:option prescient-use-char-folding :custom-variable-p t :standard t :type boolean) (:option prescient-use-case-folding :custom-variable-p t :standard smart :type (choice (const :tag "Always" t) (const :tag "Never" nil) (const :tag "Unless using upper-case letters" smart))) (:option prescient-aggressive-file-save :custom-variable-p t :standard nil :type boolean)))"#
        ]],
    )
}

/// Filtering: the three default methods match literal substrings,
/// regexps, and initialisms; the regexp builder exposes the grouped and
/// separated forms.
fn filtering_matches_literals_regexps_and_initialisms() -> ParityBatchCase {
    ParityBatchCase::value(
        "filtering_matches_literals_regexps_and_initialisms",
        r####"(unwind-protect
    (progn
      (pr564-test-reset)
      (let ((candidates '("emacs-lisp-mode"
                          "enable-local-variables"
                          "eval-last-sexp"
                          "lisp-mode"
                          "erlang-mode")))
        (list
         :regexps (prescient-filter-regexps "elm")
         :regexps-grouped (prescient-filter-regexps "elm" t)
         :regexps-separated (prescient-filter-regexps "elm" nil t)
         :literal (prescient-filter "lisp-mode" candidates)
         :regexp (prescient-filter "lisp.*mode" candidates)
         :initialism (prescient-filter "elm" candidates)
         :multi-word (prescient-filter "mode lisp" candidates)
         :no-match (prescient-filter "zzz" candidates))))
  (pr564-test-reset))"####,
        expect![[
            r#"OK (:regexps ("\\(?:e[̀-̄̆-̧̨̣̭̰̉̌̏̑]\\|[eè-ëēĕėęěȅȇȩᵉḕḗḙḛḝẹẻẽếềểễệₑℯⅇⓔｅ𝐞𝑒𝒆𝓮𝔢𝕖𝖊𝖾𝗲𝘦𝙚𝚎]\\)\\(?:\\(?:l[̧̣̭̱́̌]\\|[lĺļľˡḷḹḻḽₗℓⅼⓛｌ𝐥𝑙𝒍𝓁𝓵𝔩𝕝𝖑𝗅𝗹𝘭𝙡𝚕]\\)\\(?:m[̣́̇]\\|[mᵐḿṁṃₘⅿⓜｍ𝐦𝑚𝒎𝓂𝓶𝔪𝕞𝖒𝗆𝗺𝘮𝙢𝚖]\\)\\|㏐\\)\\|elm\\|\\be\\w*\\W*\\bl\\w*\\W*\\bm\\w*") :regexps-grouped ("\\(?:e[̀-̄̆-̧̨̣̭̰̉̌̏̑]\\|[eè-ëēĕėęěȅȇȩᵉḕḗḙḛḝẹẻẽếềểễệₑℯⅇⓔｅ𝐞𝑒𝒆𝓮𝔢𝕖𝖊𝖾𝗲𝘦𝙚𝚎]\\)\\(?:\\(?:l[̧̣̭̱́̌]\\|[lĺļľˡḷḹḻḽₗℓⅼⓛｌ𝐥𝑙𝒍𝓁𝓵𝔩𝕝𝖑𝗅𝗹𝘭𝙡𝚕]\\)\\(?:m[̣́̇]\\|[mᵐḿṁṃₘⅿⓜｍ𝐦𝑚𝒎𝓂𝓶𝔪𝕞𝖒𝗆𝗺𝘮𝙢𝚖]\\)\\|㏐\\)\\|elm\\|\\b\\(e\\)\\w*\\W*\\b\\(l\\)\\w*\\W*\\b\\(m\\)\\w*") :regexps-separated ("\\(?:e[̀-̄̆-̧̨̣̭̰̉̌̏̑]\\|[eè-ëēĕėęěȅȇȩᵉḕḗḙḛḝẹẻẽếềểễệₑℯⅇⓔｅ𝐞𝑒𝒆𝓮𝔢𝕖𝖊𝖾𝗲𝘦𝙚𝚎]\\)\\(?:\\(?:l[̧̣̭̱́̌]\\|[lĺļľˡḷḹḻḽₗℓⅼⓛｌ𝐥𝑙𝒍𝓁𝓵𝔩𝕝𝖑𝗅𝗹𝘭𝙡𝚕]\\)\\(?:m[̣́̇]\\|[mᵐḿṁṃₘⅿⓜｍ𝐦𝑚𝒎𝓂𝓶𝔪𝕞𝖒𝗆𝗺𝘮𝙢𝚖]\\)\\|㏐\\)" "elm" "\\be\\w*\\W*\\bl\\w*\\W*\\bm\\w*") :literal (#("emacs-lisp-mode" 0 15 (:prescient-match-regexps ("\\(?:l[̧̣̭̱́̌]\\|[lĺļľˡḷḹḻḽₗℓⅼⓛｌ𝐥𝑙𝒍𝓁𝓵𝔩𝕝𝖑𝗅𝗹𝘭𝙡𝚕]\\)\\(?:i[̀-̨̣̰̄̆̈̉̌̏̑]\\|[iì-ïĩīĭįǐȉȋᵢḭḯỉịⁱℹⅈⅰⓘｉ𝐢𝑖𝒊𝒾𝓲𝔦𝕚𝖎𝗂𝗶𝘪𝙞𝚒]\\)\\(?:s[̧̣̦́̂̇̌]\\|[sśŝşšſșˢṡṣṥṧṩẛₛⓢﬅｓ𝐬𝑠𝒔𝓈𝓼𝔰𝕤𝖘𝗌𝘀𝘴𝙨𝚜]\\)\\(?:p[́̇]\\|[pᵖṕṗₚⓟｐ𝐩𝑝𝒑𝓅𝓹𝔭𝕡𝖕𝗉𝗽𝘱𝙥𝚙]\\)[﹣－-]\\(?:m[̣́̇]\\|[mᵐḿṁṃₘⅿⓜｍ𝐦𝑚𝒎𝓂𝓶𝔪𝕞𝖒𝗆𝗺𝘮𝙢𝚖]\\)\\(?:o[̀-̄̆-̨̛̣̉̋̌̏̑]\\|[oºò-öōŏőơǒǫǭȍȏȫȭȯȱᵒṍṏṑṓọỏốồổỗộớờởỡợₒℴⓞｏ𝐨𝑜𝒐𝓸𝔬𝕠𝖔𝗈𝗼𝘰𝙤𝚘]\\)\\(?:d[̧̣̭̱̇̌]\\|[dďᵈḋḍḏḑḓⅆⅾⓓｄ𝐝𝑑𝒅𝒹𝓭𝔡𝕕𝖉𝖽𝗱𝘥𝙙𝚍]\\)\\(?:e[̀-̄̆-̧̨̣̭̰̉̌̏̑]\\|[eè-ëēĕėęěȅȇȩᵉḕḗḙḛḝẹẻẽếềểễệₑℯⅇⓔｅ𝐞𝑒𝒆𝓮𝔢𝕖𝖊𝖾𝗲𝘦𝙚𝚎]\\)\\|lisp-mode\\|\\bl\\w*\\W*\\bi\\w*\\W*\\bs\\w*\\W*\\bp\\w*\\W*\\b-\\w*\\W*\\bm\\w*\\W*\\bo\\w*\\W*\\bd\\w*\\W*\\be\\w*") :prescient-all-regexps ("\\(?:l[̧̣̭̱́̌]\\|[lĺļľˡḷḹḻḽₗℓⅼⓛｌ𝐥𝑙𝒍𝓁𝓵𝔩𝕝𝖑𝗅𝗹𝘭𝙡𝚕]\\)\\(?:i[̀-̨̣̰̄̆̈̉̌̏̑]\\|[iì-ïĩīĭįǐȉȋᵢḭḯỉịⁱℹⅈⅰⓘｉ𝐢𝑖𝒊𝒾𝓲𝔦𝕚𝖎𝗂𝗶𝘪𝙞𝚒]\\)\\(?:s[̧̣̦́̂̇̌]\\|[sśŝşšſșˢṡṣṥṧṩẛₛⓢﬅｓ𝐬𝑠𝒔𝓈𝓼𝔰𝕤𝖘𝗌𝘀𝘴𝙨𝚜]\\)\\(?:p[́̇]\\|[pᵖṕṗₚⓟｐ𝐩𝑝𝒑𝓅𝓹𝔭𝕡𝖕𝗉𝗽𝘱𝙥𝚙]\\)[﹣－-]\\(?:m[̣́̇]\\|[mᵐḿṁṃₘⅿⓜｍ𝐦𝑚𝒎𝓂𝓶𝔪𝕞𝖒𝗆𝗺𝘮𝙢𝚖]\\)\\(?:o[̀-̄̆-̨̛̣̉̋̌̏̑]\\|[oºò-öōŏőơǒǫǭȍȏȫȭȯȱᵒṍṏṑṓọỏốồổỗộớờởỡợₒℴⓞｏ𝐨𝑜𝒐𝓸𝔬𝕠𝖔𝗈𝗼𝘰𝙤𝚘]\\)\\(?:d[̧̣̭̱̇̌]\\|[dďᵈḋḍḏḑḓⅆⅾⓓｄ𝐝𝑑𝒅𝒹𝓭𝔡𝕕𝖉𝖽𝗱𝘥𝙙𝚍]\\)\\(?:e[̀-̄̆-̧̨̣̭̰̉̌̏̑]\\|[eè-ëēĕėęěȅȇȩᵉḕḗḙḛḝẹẻẽếềểễệₑℯⅇⓔｅ𝐞𝑒𝒆𝓮𝔢𝕖𝖊𝖾𝗲𝘦𝙚𝚎]\\)" "lisp-mode" "\\bl\\w*\\W*\\bi\\w*\\W*\\bs\\w*\\W*\\bp\\w*\\W*\\b-\\w*\\W*\\bm\\w*\\W*\\bo\\w*\\W*\\bd\\w*\\W*\\be\\w*") :prescient-ignore-case t :prescient-query "lisp-mode")) "lisp-mode") :regexp (#("emacs-lisp-mode" 0 15 (:prescient-match-regexps ("\\(?:l[̧̣̭̱́̌]\\|[lĺļľˡḷḹḻḽₗℓⅼⓛｌ𝐥𝑙𝒍𝓁𝓵𝔩𝕝𝖑𝗅𝗹𝘭𝙡𝚕]\\)\\(?:i[̀-̨̣̰̄̆̈̉̌̏̑]\\|[iì-ïĩīĭįǐȉȋᵢḭḯỉịⁱℹⅈⅰⓘｉ𝐢𝑖𝒊𝒾𝓲𝔦𝕚𝖎𝗂𝗶𝘪𝙞𝚒]\\)\\(?:s[̧̣̦́̂̇̌]\\|[sśŝşšſșˢṡṣṥṧṩẛₛⓢﬅｓ𝐬𝑠𝒔𝓈𝓼𝔰𝕤𝖘𝗌𝘀𝘴𝙨𝚜]\\)\\(?:p[́̇]\\|[pᵖṕṗₚⓟｐ𝐩𝑝𝒑𝓅𝓹𝔭𝕡𝖕𝗉𝗽𝘱𝙥𝚙]\\)[.․︙︰﹒．][*﹡＊]\\(?:m[̣́̇]\\|[mᵐḿṁṃₘⅿⓜｍ𝐦𝑚𝒎𝓂𝓶𝔪𝕞𝖒𝗆𝗺𝘮𝙢𝚖]\\)\\(?:o[̀-̄̆-̨̛̣̉̋̌̏̑]\\|[oºò-öōŏőơǒǫǭȍȏȫȭȯȱᵒṍṏṑṓọỏốồổỗộớờởỡợₒℴⓞｏ𝐨𝑜𝒐𝓸𝔬𝕠𝖔𝗈𝗼𝘰𝙤𝚘]\\)\\(?:d[̧̣̭̱̇̌]\\|[dďᵈḋḍḏḑḓⅆⅾⓓｄ𝐝𝑑𝒅𝒹𝓭𝔡𝕕𝖉𝖽𝗱𝘥𝙙𝚍]\\)\\(?:e[̀-̄̆-̧̨̣̭̰̉̌̏̑]\\|[eè-ëēĕėęěȅȇȩᵉḕḗḙḛḝẹẻẽếềểễệₑℯⅇⓔｅ𝐞𝑒𝒆𝓮𝔢𝕖𝖊𝖾𝗲𝘦𝙚𝚎]\\)\\|lisp.*mode\\|\\bl\\w*\\W*\\bi\\w*\\W*\\bs\\w*\\W*\\bp\\w*\\W*\\b\\.\\w*\\W*\\b\\*\\w*\\W*\\bm\\w*\\W*\\bo\\w*\\W*\\bd\\w*\\W*\\be\\w*") :prescient-all-regexps ("\\(?:l[̧̣̭̱́̌]\\|[lĺļľˡḷḹḻḽₗℓⅼⓛｌ𝐥𝑙𝒍𝓁𝓵𝔩𝕝𝖑𝗅𝗹𝘭𝙡𝚕]\\)\\(?:i[̀-̨̣̰̄̆̈̉̌̏̑]\\|[iì-ïĩīĭįǐȉȋᵢḭḯỉịⁱℹⅈⅰⓘｉ𝐢𝑖𝒊𝒾𝓲𝔦𝕚𝖎𝗂𝗶𝘪𝙞𝚒]\\)\\(?:s[̧̣̦́̂̇̌]\\|[sśŝşšſșˢṡṣṥṧṩẛₛⓢﬅｓ𝐬𝑠𝒔𝓈𝓼𝔰𝕤𝖘𝗌𝘀𝘴𝙨𝚜]\\)\\(?:p[́̇]\\|[pᵖṕṗₚⓟｐ𝐩𝑝𝒑𝓅𝓹𝔭𝕡𝖕𝗉𝗽𝘱𝙥𝚙]\\)[.․︙︰﹒．][*﹡＊]\\(?:m[̣́̇]\\|[mᵐḿṁṃₘⅿⓜｍ𝐦𝑚𝒎𝓂𝓶𝔪𝕞𝖒𝗆𝗺𝘮𝙢𝚖]\\)\\(?:o[̀-̄̆-̨̛̣̉̋̌̏̑]\\|[oºò-öōŏőơǒǫǭȍȏȫȭȯȱᵒṍṏṑṓọỏốồổỗộớờởỡợₒℴⓞｏ𝐨𝑜𝒐𝓸𝔬𝕠𝖔𝗈𝗼𝘰𝙤𝚘]\\)\\(?:d[̧̣̭̱̇̌]\\|[dďᵈḋḍḏḑḓⅆⅾⓓｄ𝐝𝑑𝒅𝒹𝓭𝔡𝕕𝖉𝖽𝗱𝘥𝙙𝚍]\\)\\(?:e[̀-̄̆-̧̨̣̭̰̉̌̏̑]\\|[eè-ëēĕėęěȅȇȩᵉḕḗḙḛḝẹẻẽếềểễệₑℯⅇⓔｅ𝐞𝑒𝒆𝓮𝔢𝕖𝖊𝖾𝗲𝘦𝙚𝚎]\\)" "lisp.*mode" "\\bl\\w*\\W*\\bi\\w*\\W*\\bs\\w*\\W*\\bp\\w*\\W*\\b\\.\\w*\\W*\\b\\*\\w*\\W*\\bm\\w*\\W*\\bo\\w*\\W*\\bd\\w*\\W*\\be\\w*") :prescient-ignore-case t :prescient-query "lisp.*mode")) "lisp-mode") :initialism (#("emacs-lisp-mode" 0 15 (:prescient-match-regexps ("\\(?:e[̀-̄̆-̧̨̣̭̰̉̌̏̑]\\|[eè-ëēĕėęěȅȇȩᵉḕḗḙḛḝẹẻẽếềểễệₑℯⅇⓔｅ𝐞𝑒𝒆𝓮𝔢𝕖𝖊𝖾𝗲𝘦𝙚𝚎]\\)\\(?:\\(?:l[̧̣̭̱́̌]\\|[lĺļľˡḷḹḻḽₗℓⅼⓛｌ𝐥𝑙𝒍𝓁𝓵𝔩𝕝𝖑𝗅𝗹𝘭𝙡𝚕]\\)\\(?:m[̣́̇]\\|[mᵐḿṁṃₘⅿⓜｍ𝐦𝑚𝒎𝓂𝓶𝔪𝕞𝖒𝗆𝗺𝘮𝙢𝚖]\\)\\|㏐\\)\\|elm\\|\\be\\w*\\W*\\bl\\w*\\W*\\bm\\w*") :prescient-all-regexps ("\\(?:e[̀-̄̆-̧̨̣̭̰̉̌̏̑]\\|[eè-ëēĕėęěȅȇȩᵉḕḗḙḛḝẹẻẽếềểễệₑℯⅇⓔｅ𝐞𝑒𝒆𝓮𝔢𝕖𝖊𝖾𝗲𝘦𝙚𝚎]\\)\\(?:\\(?:l[̧̣̭̱́̌]\\|[lĺļľˡḷḹḻḽₗℓⅼⓛｌ𝐥𝑙𝒍𝓁𝓵𝔩𝕝𝖑𝗅𝗹𝘭𝙡𝚕]\\)\\(?:m[̣́̇]\\|[mᵐḿṁṃₘⅿⓜｍ𝐦𝑚𝒎𝓂𝓶𝔪𝕞𝖒𝗆𝗺𝘮𝙢𝚖]\\)\\|㏐\\)" "elm" "\\be\\w*\\W*\\bl\\w*\\W*\\bm\\w*") :prescient-ignore-case t :prescient-query "elm"))) :multi-word (#("emacs-lisp-mode" 0 15 (:prescient-match-regexps ("\\(?:m[̣́̇]\\|[mᵐḿṁṃₘⅿⓜｍ𝐦𝑚𝒎𝓂𝓶𝔪𝕞𝖒𝗆𝗺𝘮𝙢𝚖]\\)\\(?:o[̀-̄̆-̨̛̣̉̋̌̏̑]\\|[oºò-öōŏőơǒǫǭȍȏȫȭȯȱᵒṍṏṑṓọỏốồổỗộớờởỡợₒℴⓞｏ𝐨𝑜𝒐𝓸𝔬𝕠𝖔𝗈𝗼𝘰𝙤𝚘]\\)\\(?:d[̧̣̭̱̇̌]\\|[dďᵈḋḍḏḑḓⅆⅾⓓｄ𝐝𝑑𝒅𝒹𝓭𝔡𝕕𝖉𝖽𝗱𝘥𝙙𝚍]\\)\\(?:e[̀-̄̆-̧̨̣̭̰̉̌̏̑]\\|[eè-ëēĕėęěȅȇȩᵉḕḗḙḛḝẹẻẽếềểễệₑℯⅇⓔｅ𝐞𝑒𝒆𝓮𝔢𝕖𝖊𝖾𝗲𝘦𝙚𝚎]\\)\\|mode\\|\\bm\\w*\\W*\\bo\\w*\\W*\\bd\\w*\\W*\\be\\w*" "\\(?:l[̧̣̭̱́̌]\\|[lĺļľˡḷḹḻḽₗℓⅼⓛｌ𝐥𝑙𝒍𝓁𝓵𝔩𝕝𝖑𝗅𝗹𝘭𝙡𝚕]\\)\\(?:i[̀-̨̣̰̄̆̈̉̌̏̑]\\|[iì-ïĩīĭįǐȉȋᵢḭḯỉịⁱℹⅈⅰⓘｉ𝐢𝑖𝒊𝒾𝓲𝔦𝕚𝖎𝗂𝗶𝘪𝙞𝚒]\\)\\(?:s[̧̣̦́̂̇̌]\\|[sśŝşšſșˢṡṣṥṧṩẛₛⓢﬅｓ𝐬𝑠𝒔𝓈𝓼𝔰𝕤𝖘𝗌𝘀𝘴𝙨𝚜]\\)\\(?:p[́̇]\\|[pᵖṕṗₚⓟｐ𝐩𝑝𝒑𝓅𝓹𝔭𝕡𝖕𝗉𝗽𝘱𝙥𝚙]\\)\\|lisp\\|\\bl\\w*\\W*\\bi\\w*\\W*\\bs\\w*\\W*\\bp\\w*") :prescient-all-regexps ("\\(?:m[̣́̇]\\|[mᵐḿṁṃₘⅿⓜｍ𝐦𝑚𝒎𝓂𝓶𝔪𝕞𝖒𝗆𝗺𝘮𝙢𝚖]\\)\\(?:o[̀-̄̆-̨̛̣̉̋̌̏̑]\\|[oºò-öōŏőơǒǫǭȍȏȫȭȯȱᵒṍṏṑṓọỏốồổỗộớờởỡợₒℴⓞｏ𝐨𝑜𝒐𝓸𝔬𝕠𝖔𝗈𝗼𝘰𝙤𝚘]\\)\\(?:d[̧̣̭̱̇̌]\\|[dďᵈḋḍḏḑḓⅆⅾⓓｄ𝐝𝑑𝒅𝒹𝓭𝔡𝕕𝖉𝖽𝗱𝘥𝙙𝚍]\\)\\(?:e[̀-̄̆-̧̨̣̭̰̉̌̏̑]\\|[eè-ëēĕėęěȅȇȩᵉḕḗḙḛḝẹẻẽếềểễệₑℯⅇⓔｅ𝐞𝑒𝒆𝓮𝔢𝕖𝖊𝖾𝗲𝘦𝙚𝚎]\\)" "mode" "\\bm\\w*\\W*\\bo\\w*\\W*\\bd\\w*\\W*\\be\\w*" "\\(?:l[̧̣̭̱́̌]\\|[lĺļľˡḷḹḻḽₗℓⅼⓛｌ𝐥𝑙𝒍𝓁𝓵𝔩𝕝𝖑𝗅𝗹𝘭𝙡𝚕]\\)\\(?:i[̀-̨̣̰̄̆̈̉̌̏̑]\\|[iì-ïĩīĭįǐȉȋᵢḭḯỉịⁱℹⅈⅰⓘｉ𝐢𝑖𝒊𝒾𝓲𝔦𝕚𝖎𝗂𝗶𝘪𝙞𝚒]\\)\\(?:s[̧̣̦́̂̇̌]\\|[sśŝşšſșˢṡṣṥṧṩẛₛⓢﬅｓ𝐬𝑠𝒔𝓈𝓼𝔰𝕤𝖘𝗌𝘀𝘴𝙨𝚜]\\)\\(?:p[́̇]\\|[pᵖṕṗₚⓟｐ𝐩𝑝𝒑𝓅𝓹𝔭𝕡𝖕𝗉𝗽𝘱𝙥𝚙]\\)" "lisp" "\\bl\\w*\\W*\\bi\\w*\\W*\\bs\\w*\\W*\\bp\\w*") :prescient-ignore-case t :prescient-query "mode lisp")) "lisp-mode") :no-match nil)"#
        ]],
    )
}

/// Sorting: unmatched-length ordering with shorter candidates first, the
/// recency boost for remembered candidates, and the frequency boost that
/// accumulates over repeated `prescient-remember' calls.
fn sorting_prefers_shorter_recent_and_frequent_candidates() -> ParityBatchCase {
    ParityBatchCase::value(
        "sorting_prefers_shorter_recent_and_frequent_candidates",
        r####"(unwind-protect
    (progn
      (pr564-test-reset)
      (let ((candidates '("eval-last-sexp"
                          "erlang-mode"
                          "emacs-lisp-mode"
                          "lisp-mode")))
        (list
         :by-length (prescient-sort candidates)
         :after-remember
         (progn
           (prescient-remember "erlang-mode")
           (prescient-sort candidates))
         :after-frequency
         (progn
           (dotimes (_ 10) (prescient-remember "lisp-mode"))
           (prescient-sort candidates)))))
  (pr564-test-reset))"####,
        expect![[
            r#"OK (:by-length #1=("lisp-mode" "erlang-mode" "eval-last-sexp" "emacs-lisp-mode") :after-remember #1# :after-frequency #1#)"#
        ]],
    )
}

/// The persistence round trip: `prescient--save' writes the history,
/// frequency, and serial number through the cache-callback form into the
/// save file, and `prescient--load' restores the exact tables into a
/// cleared cache.
fn the_cache_persists_and_reads_back() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_cache_persists_and_reads_back",
        r####"(unwind-protect
    (progn
      (pr564-test-reset)
      (let ((save-file (expand-file-name
                        "prescient-save.el.~1~"
                        (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
        (setq prescient-save-file save-file)
        (prescient-remember "frequent-candidate")
        (dotimes (_ 5) (prescient-remember "very-frequent"))
        (let ((history-before (hash-table-count prescient--history))
              (frequency-before (hash-table-count prescient--frequency))
              (serial-before prescient--serial-number))
          (prescient--save)
          (let ((written (file-exists-p save-file)))
            (setq prescient--history (make-hash-table :test 'equal)
                  prescient--frequency (make-hash-table :test 'equal)
                  prescient--serial-number 0
                  prescient--cache-loaded nil)
            (prescient--load)
            (list :history-before history-before
                  :frequency-before frequency-before
                  :serial-before serial-before
                  :written written
                  :history-loaded (hash-table-count prescient--history)
                  :frequency-loaded (hash-table-count prescient--frequency)
                  :serial-loaded prescient--serial-number
                  :frequent (gethash "frequent-candidate"
                                     prescient--frequency)
                  :very-frequent (gethash "very-frequent"
                                          prescient--frequency))))))
  (pr564-test-reset))"####,
        expect![
            "OK (:history-before 2 :frequency-before 2 :serial-before 6 :written t :history-loaded 2 :frequency-loaded 2 :serial-loaded 6 :frequent 0.9821344612135428 :very-frequent 4.955179595485757)"
        ],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        the_configuration_surface_and_payload(),
        filtering_matches_literals_regexps_and_initialisms(),
        sorting_prefers_shorter_recent_and_frequent_candidates(),
        the_cache_persists_and_reads_back(),
    ]
}
