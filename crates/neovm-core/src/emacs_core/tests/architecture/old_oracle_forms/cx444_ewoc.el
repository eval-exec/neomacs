
(progn (require 'ewoc)
  (with-temp-buffer
    (let ((ewoc (ewoc-create 'identity))))
      (fboundp 'ewoc-enter-first)))
