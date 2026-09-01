use expect_test::expect;

use super::ParityBatchCase;

fn apache_mode_formats_comments_and_deploys_a_tls_virtual_host() -> ParityBatchCase {
    ParityBatchCase::value(
        "apache_mode_formats_comments_and_deploys_a_tls_virtual_host",
        r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "apache-mode-vhost-workflow"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (site
        (expand-file-name
         "etc/apache2/sites-available/api.conf"
         root))
       (default-directory root)
       buffer
       result)
  (unwind-protect
      (progn
        (neomacs-apache-test-cleanup root)
        (make-directory (file-name-directory site) t)
        (with-temp-file site
          (insert
           "<VirtualHost *:443>\n"
           "ServerName api.example.test\n"
           "DocumentRoot \"/srv/www/api\"\n"
           "<Directory \"/srv/www/api\">\n"
           "Options Indexes FollowSymLinks\n"
           "Require all granted\n"
           "</Directory>\n"
           "SSLEngine On\n"
           "SSLProtocol all -SSLv3\n"
           "</VirtualHost>\n"))
        (setq buffer (find-file-noselect site))
        (switch-to-buffer buffer)
        (setq-local indent-tabs-mode nil)
        (indent-region (point-min) (point-max))
        (goto-char (point-min))
        (search-forward "Options Indexes FollowSymLinks")
        (replace-match "Options -Indexes +FollowSymLinks" t t)
        (comment-region
         (line-beginning-position)
         (line-beginning-position 2))
        (goto-char (point-min))
        (search-forward "SSLProtocol all -SSLv3")
        (end-of-line)
        (insert
         "\nHeader always set Strict-Transport-Security "
         "\"max-age=31536000; includeSubDomains\"")
        (indent-region (point-min) (point-max))
        (font-lock-ensure)
        (save-buffer)
        (goto-char (point-min))
        (search-forward "Strict-Transport-Security")
        (setq result
              (list
               :file (file-relative-name buffer-file-name root)
               :mode major-mode
               :point
               (list
                (line-number-at-pos)
                (current-column)
                (buffer-substring-no-properties
                 (line-beginning-position)
                 (line-end-position)))
               :lines (neomacs-apache-test-lines)
               :faces
               (mapcar
                (lambda (token)
                  (list
                   token
                   (neomacs-apache-test-face-at token)))
                '("VirtualHost"
                  "ServerName"
                  "\"/srv/www/api\""
                  "# Options"
                  "SSLEngine"
                  "On"
                  "Header"
                  "\"max-age=31536000; includeSubDomains\""))
               :modified (buffer-modified-p)
               :disk (neomacs-apache-test-file-string site))))
    (neomacs-apache-test-cleanup root))
  result)
"####,
        expect![[
            r##"OK (:file "etc/apache2/sites-available/api.conf" :mode apache-mode :point (10 47 "    Header always set Strict-Transport-Security \"max-age=31536000; includeSubDomains\"") :lines ((1 0 "<VirtualHost *:443>") (2 4 "    ServerName api.example.test") (3 4 "    DocumentRoot \"/srv/www/api\"") (4 4 "    <Directory \"/srv/www/api\">") (5 8 "        # Options -Indexes +FollowSymLinks") (6 8 "        Require all granted") (7 4 "    </Directory>") (8 4 "    SSLEngine On") (9 4 "    SSLProtocol all -SSLv3") (10 4 "    Header always set Strict-Transport-Security \"max-age=31536000; includeSubDomains\"") (11 0 "</VirtualHost>")) :faces (("VirtualHost" font-lock-function-name-face) ("ServerName" font-lock-keyword-face) ("\"/srv/www/api\"" font-lock-string-face) ("# Options" font-lock-comment-delimiter-face) ("SSLEngine" font-lock-keyword-face) ("On" font-lock-type-face) ("Header" font-lock-keyword-face) ("\"max-age=31536000; includeSubDomains\"" font-lock-string-face)) :modified nil :disk "<VirtualHost *:443>\n    ServerName api.example.test\n    DocumentRoot \"/srv/www/api\"\n    <Directory \"/srv/www/api\">\n        # Options -Indexes +FollowSymLinks\n        Require all granted\n    </Directory>\n    SSLEngine On\n    SSLProtocol all -SSLv3\n    Header always set Strict-Transport-Security \"max-age=31536000; includeSubDomains\"\n</VirtualHost>\n")"##
        ]],
    )
}

fn apache_mode_maintains_an_authenticated_https_htaccess_policy() -> ParityBatchCase {
    ParityBatchCase::value(
        "apache_mode_maintains_an_authenticated_https_htaccess_policy",
        r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "apache-mode-htaccess-workflow"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (policy
        (expand-file-name "srv/www/admin/.htaccess" root))
       (default-directory root)
       buffer
       disabled-rewrite
       files-require-face
       result)
  (unwind-protect
      (progn
        (neomacs-apache-test-cleanup root)
        (make-directory (file-name-directory policy) t)
        (with-temp-file policy
          (insert
           "AuthType Basic\n"
           "AuthName \"Operations console\"\n"
           "AuthUserFile /etc/apache2/admin.htpasswd\n"
           "Require valid-user\n"
           "\n"
           "RewriteEngine On\n"
           "RewriteCond %{HTTPS} !=on\n"
           "RewriteRule ^ https://admin.example.test%{REQUEST_URI} [R=301,L]\n"))
        (setq buffer (find-file-noselect policy))
        (switch-to-buffer buffer)
        (goto-char (point-min))
        (search-forward "RewriteCond")
        (beginning-of-line)
        (let ((start (point)))
          (forward-line 2)
          (comment-region start (point)))
        (setq disabled-rewrite
              (buffer-substring-no-properties
               (line-beginning-position -1)
               (line-beginning-position 1)))
        (goto-char (point-min))
        (search-forward "# RewriteCond")
        (beginning-of-line)
        (let ((start (point)))
          (forward-line 2)
          (uncomment-region start (point)))
        (goto-char (point-min))
        (search-forward "AuthName \"Operations console\"")
        (replace-match "AuthName \"Production operations\"" t t)
        (goto-char (point-max))
        (insert
         "\n<FilesMatch \"^health\\.json$\">\n"
         "Require all granted\n"
         "</FilesMatch>\n")
        (setq-local indent-tabs-mode nil)
        (indent-region (point-min) (point-max))
        (font-lock-ensure)
        (save-buffer)
        (goto-char (point-min))
        (search-forward "Require all granted")
        (setq files-require-face
              (get-text-property
               (match-beginning 0)
               'face))
        (setq result
              (list
               :file (file-relative-name buffer-file-name root)
               :mode major-mode
               :disabled-rewrite disabled-rewrite
               :point
               (list
                (line-number-at-pos)
                (current-column)
                (current-indentation))
               :lines (neomacs-apache-test-lines)
               :faces
               (mapcar
                (lambda (token)
                  (list
                   token
                   (neomacs-apache-test-face-at token)))
                '("AuthType"
                  "Basic"
                  "\"Production operations\""
                  "valid-user"
                  "RewriteEngine"
                  "On"
                  "RewriteRule"
                  "FilesMatch"))
               :files-require-face files-require-face
               :modified (buffer-modified-p)
               :disk (neomacs-apache-test-file-string policy))))
    (neomacs-apache-test-cleanup root))
  result)
"####,
        expect![[
            r##"OK (:file "srv/www/admin/.htaccess" :mode apache-mode :disabled-rewrite "# RewriteCond %{HTTPS} !=on\n# RewriteRule ^ https://admin.example.test%{REQUEST_URI} [R=301,L]\n" :point (11 23 4) :lines ((1 0 "AuthType Basic") (2 0 "AuthName \"Production operations\"") (3 0 "AuthUserFile /etc/apache2/admin.htpasswd") (4 0 "Require valid-user") (5 0 "") (6 0 "RewriteEngine On") (7 0 "RewriteCond %{HTTPS} !=on") (8 0 "RewriteRule ^ https://admin.example.test%{REQUEST_URI} [R=301,L]") (9 0 "") (10 0 "<FilesMatch \"^health\\.json$\">") (11 4 "    Require all granted") (12 0 "</FilesMatch>")) :faces (("AuthType" font-lock-keyword-face) ("Basic" font-lock-type-face) ("\"Production operations\"" font-lock-string-face) ("valid-user" font-lock-type-face) ("RewriteEngine" font-lock-keyword-face) ("On" font-lock-type-face) ("RewriteRule" font-lock-keyword-face) ("FilesMatch" font-lock-function-name-face)) :files-require-face font-lock-keyword-face :modified nil :disk "AuthType Basic\nAuthName \"Production operations\"\nAuthUserFile /etc/apache2/admin.htpasswd\nRequire valid-user\n\nRewriteEngine On\nRewriteCond %{HTTPS} !=on\nRewriteRule ^ https://admin.example.test%{REQUEST_URI} [R=301,L]\n\n<FilesMatch \"^health\\.json$\">\n    Require all granted\n</FilesMatch>\n")"##
        ]],
    )
}

fn apache_mode_clones_and_customizes_a_reverse_proxy_configuration() -> ParityBatchCase {
    ParityBatchCase::value(
        "apache_mode_clones_and_customizes_a_reverse_proxy_configuration",
        r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "apache-mode-proxy-workflow"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (staging
        (expand-file-name
         "etc/httpd/conf/extra/staging-proxy.conf"
         root))
       (production
        (expand-file-name
         "etc/httpd/conf/extra/production-proxy.conf"
         root))
       (default-directory root)
       buffer
       replacements
       result)
  (unwind-protect
      (progn
        (neomacs-apache-test-cleanup root)
        (make-directory (file-name-directory staging) t)
        (with-temp-file staging
          (insert
           "<VirtualHost *:80>\n"
           "ServerName staging.example.test\n"
           "ProxyPass /app http://staging-app.internal:8080/\n"
           "ProxyPassReverse /app http://staging-app.internal:8080/\n"
           "<Location /health>\n"
           "Require all granted\n"
           "</Location>\n"
           "</VirtualHost>\n"))
        (copy-file staging production t)
        (setq buffer (find-file-noselect production))
        (switch-to-buffer buffer)
        (setq-local apache-indent-level 2)
        (setq-local indent-tabs-mode nil)
        (goto-char (point-min))
        (while
            (search-forward "staging" nil t)
          (replace-match "production" t t)
          (setq replacements (1+ (or replacements 0))))
        (goto-char (point-min))
        (search-forward "ServerName production.example.test")
        (end-of-line)
        (insert "\nProxyPreserveHost On")
        (indent-region (point-min) (point-max))
        (font-lock-ensure)
        (save-buffer)
        (goto-char (point-min))
        (search-forward "ProxyPreserveHost On")
        (setq result
              (list
               :source (neomacs-apache-test-file-string staging)
               :destination-file
               (file-relative-name buffer-file-name root)
               :mode major-mode
               :indent-level apache-indent-level
               :replacements replacements
               :point
               (list
                (line-number-at-pos)
                (current-column)
                (current-indentation))
               :lines (neomacs-apache-test-lines)
               :faces
               (mapcar
                (lambda (token)
                  (list
                   token
                   (neomacs-apache-test-face-at token)))
                '("VirtualHost"
                  "ServerName"
                  "ProxyPreserveHost"
                  "On"
                  "ProxyPass"
                  "Location"
                  "Require"
                  "all"))
               :modified (buffer-modified-p)
               :destination
               (neomacs-apache-test-file-string production))))
    (neomacs-apache-test-cleanup root))
  result)
"####,
        expect![[
            r#"OK (:source "<VirtualHost *:80>\nServerName staging.example.test\nProxyPass /app http://staging-app.internal:8080/\nProxyPassReverse /app http://staging-app.internal:8080/\n<Location /health>\nRequire all granted\n</Location>\n</VirtualHost>\n" :destination-file "etc/httpd/conf/extra/production-proxy.conf" :mode apache-mode :indent-level 2 :replacements 3 :point (3 22 2) :lines ((1 0 "<VirtualHost *:80>") (2 2 "  ServerName production.example.test") (3 2 "  ProxyPreserveHost On") (4 2 "  ProxyPass /app http://production-app.internal:8080/") (5 2 "  ProxyPassReverse /app http://production-app.internal:8080/") (6 2 "  <Location /health>") (7 4 "    Require all granted") (8 2 "  </Location>") (9 0 "</VirtualHost>")) :faces (("VirtualHost" font-lock-function-name-face) ("ServerName" font-lock-keyword-face) ("ProxyPreserveHost" font-lock-keyword-face) ("On" font-lock-type-face) ("ProxyPass" font-lock-keyword-face) ("Location" font-lock-function-name-face) ("Require" font-lock-keyword-face) ("all" font-lock-type-face)) :modified nil :destination "<VirtualHost *:80>\n  ServerName production.example.test\n  ProxyPreserveHost On\n  ProxyPass /app http://production-app.internal:8080/\n  ProxyPassReverse /app http://production-app.internal:8080/\n  <Location /health>\n    Require all granted\n  </Location>\n</VirtualHost>\n")"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        apache_mode_formats_comments_and_deploys_a_tls_virtual_host(),
        apache_mode_maintains_an_authenticated_https_htaccess_policy(),
        apache_mode_clones_and_customizes_a_reverse_proxy_configuration(),
    ]
}
