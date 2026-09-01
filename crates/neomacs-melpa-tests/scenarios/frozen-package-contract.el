;; This probe runs only after the install process has exited.  Its success
;; therefore locks in package persistence, generated autoloads, dependency
;; activation, tar extraction, and byte-compiled loading across a restart.

(unless (package-installed-p 'simple-single '(1 3))
  (error "simple-single dependency was not installed"))
(unless (package-installed-p 'simple-depend '(1 0))
  (error "simple-depend dependency was not installed"))
(unless (package-installed-p 'simple-two-depend '(1 1))
  (error "requested dependency-chain package was not installed"))
(unless (package-installed-p 'multi-file '(0 2 3))
  (error "multi-file tar package was not installed"))

(unless (fboundp 'simple-single-mode)
  (error "simple-single-mode autoload was unavailable after restart"))
(unless (fboundp 'multi-file-mode)
  (error "multi-file-mode autoload was unavailable after restart"))

(load "simple-depend")
(load "simple-two-depend")
(unless (equal simple-depend "Value")
  (error "simple-depend did not load from the installed dependency"))
(unless (equal simple-two-depend "Value")
  (error "simple-two-depend did not load after restart"))

(with-temp-buffer
  (simple-single-mode 1)
  (unless simple-single-mode
    (error "simple-single-mode autoload did not enable"))
  (multi-file-mode)
  (unless (eq major-mode 'multi-file-mode)
    (error "multi-file-mode autoload did not select the installed mode")))

'(:dependency-chain t :multi-file t :autoloads t :restart t)
